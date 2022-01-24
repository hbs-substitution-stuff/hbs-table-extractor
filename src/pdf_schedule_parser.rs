use std::collections::{HashMap, HashSet};
use std::ffi::OsStr;
use std::path::Path;
use encoding_rs::WINDOWS_1252;
use lopdf::{Document, Stream};
use std::error::Error;
use std::iter::{Filter, FilterMap};
use std::slice::Iter;
use chrono::{Local, NaiveDate, Utc, Offset};


/// The parser itself
pub struct PdfScheduleParser {
	document: Document,
	pub pages: Vec<PageStream>,
}

/// An object collection
#[derive(Clone)]
pub struct ObjectStream(Vec<PdfTableObject>);

/// The text in the pdf
#[derive(Clone, Debug)]
struct Text {
	text: String,
	position: Point,
}

/// Just a 2D point
#[derive(Clone, Debug)]
struct Point {
	x: i64,
	y: i64,
}


/// The lines in the pdf, e.g. the lines that a table is made out of
#[derive(Clone, Debug)]
struct Line {
	start: Point,
	end: Point,
}

/// All pdf objects relevant for parsing the schedule
#[derive(Clone, Debug)]
enum PdfTableObject {
	Line(Line),
	Text(Text),
}

impl PdfTableObject {
	fn in_bound(&self, bound_top: i64, bound_bottom: i64) -> bool {
		let in_bound = |o| o < bound_top && o > bound_bottom;

		match self {
			PdfTableObject::Text(t) => {
				in_bound(t.position.y)
			},
			PdfTableObject::Line(l) => {
				in_bound(l.start.y) && in_bound(l.end.y)
			},
		}
	}

	fn is_text(&self) -> bool {
		if let Self::Text(_) = self {
			true
		} else {
			false
		}
	}

	fn is_line(&self) -> bool {
		if let Self::Line(_) = self {
			true
		} else {
			false
		}
	}
}

/// The a horizontal border
type Border = i64;

/* trait PdfObject {
	fn in_bound(&self, bound_top: i64, bound_bottom: i64) -> bool;
} */


/// The pdf objects corresponding to exactly one table
pub struct TableObjects {
	vertical_borders: Vec<Border>,
	horizontal_borders: Vec<Border>,
	texts: Vec<Text>,
}

impl Line {
	fn len(&self) -> i64 {
		(
			((self.end.x - self.start.x) as f64).powi(2) +
			((self.end.y - self.start.y) as f64).powi(2)
		).sqrt() as i64
	}

	fn vertical(&self) -> bool {
		self.start.x == self.end.x
	}

	fn horizontal(&self) -> bool {
		self.start.y == self.end.y
	}
}

impl PdfScheduleParser {
	pub(crate) fn new<T: AsRef<Path> + AsRef<OsStr>>(path: T) -> Result<Self, Box<dyn std::error::Error>> {
		let document = Document::load(path).unwrap();

		let mut pages = Vec::new();

		for page in document.page_iter() {
			for object_id in document.get_page_contents(page) {
				let object = document.get_object(object_id).unwrap();

				if let Ok(stream) = object.as_stream() {
					pages.push(ObjectStream::new(stream)?);
				};
			};
		};

		Ok(Self {
			document,
			pages,
		})
	}

	pub fn extract_date(&self) -> Result<i64, Box<dyn Error>> {
		let date_string = self.pages.iter()
			.map(|p| p.texts())
			.flatten()
			.filter(|t| t.text.contains("Datum: "))
			.next()
			.ok_or("Couldn't find the date string in PDF")?
			.text
			.as_str();

		let date_begin = date_string.rfind(' ').ok_or("Date string malformed")? + 1;

		Ok(
			chrono::NaiveDate::parse_from_str(&date_string[date_begin..], "%d.%m.%Y")?
				.and_hms_milli(0, 0, 0, 0)
				.timestamp_millis()
		)
	}
}

type CellContent<'a> = Vec<&'a str>;
type ColumnarTable<'a> = Vec<Vec<CellContent<'a>>>;
type PageStream = ObjectStream;

impl TableObjects {
	fn new(objects: ObjectStream) -> Result<Self, Box<dyn Error>> {
		let mut vertical_borders: HashSet<Border> = HashSet::new();
		let mut horizontal_borders = HashMap::new();

		for line in objects.lines() {
		    if line.horizontal() {
				let mut x = horizontal_borders.entry(line.start.y)
					.or_insert(Vec::new());
				x.push(line.start.x);
				x.push(line.end.x);
		    } else if line.vertical() {
		        vertical_borders.insert(line.start.x);
		    } else {
		        return Err("While parsing pdf: line is diagonal".into())
		    };
		};

		let mut horizontal_borders = horizontal_borders.iter()
			.map(|l| {
				//println!("{:?}", l.0);
				//println!("{:?}", l.1);
				let start = Point {x: *l.1.iter().min().unwrap(), y: *l.0};
				let end = Point {x: *l.1.iter().max().unwrap(), y: *l.0};
				Line {start, end}
			})
			.collect::<Vec<Line>>();

		//let max_length = horizontal_borders.iter()
		//	.map(|l| l.len())
		//	.max()
		//	.ok_or("couldn't find any horizontal boarders")?;

		//let horizontal_borders = horizontal_borders.iter()
		//	.filter(|l| l.len() + 5 > (max_length + 10))
		//	.map(|l| l.start.y)
		//	.collect::<Vec<Border>>();

		horizontal_borders.sort_by(|l1, l2| l2.len().cmp(&l1.len()));

		//println!("{:?}", horizontal_borders.iter().map(|l| l.len()).collect::<Vec<i64>>());

		horizontal_borders.truncate(7);

		let horizontal_borders = horizontal_borders.drain(..).map(|l| l.start.y);


		//println!("{}", vertical_borders.len());

		if horizontal_borders.len() != 7 {
			return Err(format!("number of horizontal lines is expected to exactly 7, got {}", horizontal_borders.len()).into());
		}

		//println!("{:?}", vertical_borders);

		//TODO add a check to check the vertical borders against the number of classes

		let vertical_borders = vertical_borders.into_iter();

		Ok(Self {
			vertical_borders: vertical_borders.collect(),
			horizontal_borders: horizontal_borders.collect(),
			texts: objects.texts().map(|t| t.clone()).collect(),
		})
	}

	pub fn generate_table(&mut self) -> Result<ColumnarTable, Box<dyn Error>> {
		self.horizontal_borders.sort();
		self.horizontal_borders.reverse();
		self.vertical_borders.sort();

		//println!("{:?}", self.horizontal_borders);
		//println!("{:?}", self.vertical_borders);

		let mut table: ColumnarTable = vec![vec![Vec::new(); 7]; self.vertical_borders.len() - 1];

		for text in &self.texts {
			for (v_idx, v_border) in self.vertical_borders.iter().enumerate() {
				if text.position.x < *v_border {
					for (h_idx, h_border) in self.horizontal_borders.iter().enumerate() {
						if text.position.y > *h_border {
							table[v_idx - 1][h_idx].push(&text.text);
							break
						}
					}
					break
				}
			}
		}

		println!("{:?}", table);

		Ok(table)
	}
}

impl ObjectStream {
	fn new(stream: &Stream) -> Result<Self, Box<dyn std::error::Error>> {
		let mut stream = stream.to_owned();
		stream.decompress();
		let stream = stream.decode_content().unwrap();

		let mut objects = Vec::new();


		//find all Tj's and their position through the previous Td's and put them as a Text struct in an array
		//find all l's and their position through the previous m's and put them as a Line struct in an array
		for (i, op) in stream.operations.iter().enumerate() {
			match op.operator.as_str() {
				"Tj" => {
					let td = &stream.operations[i - 1];

					if td.operator == "Td" {
						let td_ops = &td.operands;
						let tj_ops = &op.operands;
						//TODO maybe use Document::decode_text();
						// let text = WINDOWS_1252.decode(tj_ops[0].as_str().unwrap()).0
						// 	.parse()
						// 	.unwrap();

						let text = Document::decode_text(
							Some("WinAnsiEncoding"),
							tj_ops[0].as_str().unwrap()
						);

						let position = Point {
							x: td_ops[0].as_f64().unwrap() as i64,
							y: td_ops[1].as_f64().unwrap() as i64,
						};

						objects.push(PdfTableObject::Text(Text {
							text,
							position
						}));
					} else {
						return Err("While parsing pdf: Td expjected before Tj".into());
					}
				}
				"l" => {
					let m = &stream.operations[i - 1];

					if m.operator == "m" {
						let m_ops = &m.operands;
						let l_ops = &op.operands;

						let start = Point {
							x: m_ops[0].as_f64().unwrap() as i64,
							y: m_ops[1].as_f64().unwrap() as i64,
						};

						let end = Point {
							x: l_ops[0].as_f64().unwrap() as i64,
							y: l_ops[1].as_f64().unwrap() as i64,
						};

						objects.push(PdfTableObject::Line(
							Line {
								start,
								end,
							}
						))
					} else {
						return Err("While parsing pdf: m expected before l".into());
					}
				}
				_ => (),
			}
		}

		Ok(Self(objects))
	}

	fn lines<'a>(&'a self) -> FilterMap<Iter<'_, PdfTableObject>, fn(&'a PdfTableObject) -> Option<&'a Line>> {
		self.0.iter().filter_map(|o| if let PdfTableObject::Line(l) = o {Some(l)} else {None})
	}

	fn texts<'a>(&'a self) -> FilterMap<Iter<'_, PdfTableObject>, fn(&'a PdfTableObject) -> Option<&'a Text>> {
		self.0.iter().filter_map(|o| if let PdfTableObject::Text(t) = o {Some(t)} else {None})
	}

	pub fn extract_table_objects(&mut self) -> Vec<TableObjects> {
		//TODO get the regions of the tables, by finding the coordinates of the "Block" text and
		// use it to divide the lines on the page. Maybe even find the line below
		// '5:  15:15\n- 16:45' as the second divider

		let mut top_limit_y = self.texts()
			.filter(|t| t.text == "Block")
			.map(|t| t.position.y + 4 /* add a tolerance of 4 */)
			.collect::<Vec<i64>>();

		top_limit_y.sort();

		let mut bottom_limit_y = self.texts()
			.filter(|t| t.text.contains("15:15"))
			.map(|t| t.position.y)
			.collect::<Vec<i64>>();

		bottom_limit_y.sort();

		if bottom_limit_y.len() != top_limit_y.len() {
			panic!("bottom and top limits don't match up");
		}

		//TODO adjust bottom_limit to extend to the bottom line and add a tolerance
		let mut line_deltas = Vec::new();

		for limit in &bottom_limit_y {
			line_deltas.push(self.lines().filter_map(|line| {
				let delta = line.start.y - limit;

				if line.horizontal() && delta.is_negative() {
					Some(delta)
				} else {
					None
				}
			}).max().unwrap())
		}

		if line_deltas.len() != bottom_limit_y.len() {
			panic!("something went wrong")
		}

		let mut line_deltas = line_deltas.into_iter();

		let bottom_limit_y = bottom_limit_y.drain(..)
			.map(|l| l + line_deltas.next().unwrap() - 4 /* add a tolerance of -4 */)
			.collect::<Vec<i64>>();

		//TODO make a function which takes a two limits and the table objects and returns only objects in bound

		let mut extracted_tables = vec![ObjectStream(Vec::new()); top_limit_y.len()];

		for object in self.0.drain(..) {
			for (idx, (top_bound, bottom_bound)) in top_limit_y.iter().zip(&bottom_limit_y).enumerate() {
				if object.in_bound(*top_bound, *bottom_bound) {
					extracted_tables[idx].0.push(object.clone());
				}
			}
		}

		//println!("{:?}", extracted_tables.drain(..).map(|a| a.0).collect::<Vec<Vec<PdfTableObject>>>().iter().map(|o| o.len()).collect::<Vec<usize>>());

		extracted_tables.drain(..)
			.map(|o| TableObjects::new(o).unwrap())
			.collect()
	}
}
