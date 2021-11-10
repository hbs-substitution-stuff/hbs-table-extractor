use std::collections::HashMap;
use std::ffi::OsStr;
use std::path::Path;
use encoding_rs::WINDOWS_1252;
use lopdf::{Document, Stream};
use std::error::Error;
use chrono::{Local, NaiveDate, Utc, Offset};

pub struct PdfScheduleParser {
	document: Document,
	pages: Vec<PageStream>,
}

struct PageStream {
	lines: Vec<Line>,
	texts: Vec<Text>,
}

struct Text {
	text: String,
	position: Point,
}

struct Point {
	x: i64,
	y: i64,
}

struct Line {
	start: Point,
	end: Point,
}

type Border = i64;

struct TableObjects {
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
}

impl PdfScheduleParser {
	pub(crate) fn new<T: AsRef<Path> + AsRef<OsStr>>(path: T) -> Result<Self, Box<dyn std::error::Error>> {
		let document = Document::load(path).unwrap();

		let mut pages = Vec::new();

		for page in document.page_iter() {
			for object_id in document.get_page_contents(page) {
				let object = document.get_object(object_id).unwrap();

				if let Ok(stream) = object.as_stream() {
					pages.push(PageStream::new(stream)?);
				};
			};
		};

		Ok(Self {
			document,
			pages,
		})
	}

	fn extract_date(&self) -> Result<i64, Box<dyn Error>> {
		let date_string = self.pages.iter()
			.map(|p| p.texts)
			.map(|t| {
				t.iter().map(|t| t.text.starts_with("Datum: "))
			}).next()
			.ok_or("Couldn't find the date string in PDF")?
			.collect::<&str>();

		println!(date_string);

		let date_begin = date_string.rfind(' ').ok_or("Date string malformed")?;

		Ok(
			chrono::NaiveDateTime::parse_from_str(&date_string[date_begin..], "%d.%m.%Y")?
				.timestamp_millis()
		)
	}
}

type CellContent<'a> = Vec<&'a str>;
type ColumnarTable<'a> = Vec<Vec<CellContent<'a>>>;

impl TableObjects {
	fn new(objects: PageStream) -> Result<Self, Box<dyn Error>> {
		let mut vertical_borders: Vec<Border> = Vec::new();
		let mut horizontal_borders = HashMap::new();

		for line in objects.lines {
		    if line.start.y == line.end.y {
				horizontal_borders.entry(line.start.y).and_modify(|x: &mut Vec<i64>| {
					x.push(line.start.x);
					x.push(line.end.x)
				}).or_insert(Vec::new());
		    } else if line.start.x == line.end.x {
		        vertical_borders.push(line.start.y)
		    } else {
		        return Err("While parsing pdf: line is diagonal".into())
		    };
		};

		let mut horizontal_borders = horizontal_borders.iter()
			.map(|l| {
				let start = Point::new(*l.1.iter().min().unwrap(), *l.0);
				let end = Point::new(*l.1.iter().max().unwrap(), *l.0);
				Line::new(start, end)
			})
			.collect::<Vec<Line>>();

		let max_length = horizontal_borders.iter()
			.map(|l| l.len())
			.max()
			.ok_or("couldn't find any horizontal boarders")?;

		let horizontal_borders = horizontal_borders.iter()
			.filter(|l| l.len() + 5 > (max_length + 10))
			.map(|l| l.start.y)
			.collect::<Vec<Border>>();

		if horizontal_borders.len() != 7 {
			Err(format!("number of horizontal lines is expected to exactly 7, got {}", horizontal_borders.len()).into())
		}

		Ok(Self {
			vertical_borders,
			horizontal_borders,
			texts: objects.texts,
		})
	}

	fn generate_table(&mut self) -> Result<ColumnarTable, Box<dyn Error>> {
		self.horizontal_borders.sort();
		self.horizontal_borders.reverse();
		self.vertical_borders.sort();

		println!("{:?}", self.horizontal_borders);
		println!("{:?}", self.vertical_borders);

		let mut table: ColumnarTable = vec![vec![Vec::new(); 7]; x_borders.len() - 1];

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

impl PageStream {
	fn new(stream: &Stream) -> Result<Self, Box<dyn std::error::Error>> {
		let mut stream = stream.to_owned();
		stream.decompress();
		let stream = stream.decode_content().unwrap();

		let mut texts = Vec::new();
		let mut lines = Vec::new();


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

						texts.push(Text {
							text,
							position
						});
					} else {
						return Err("While parsing pdf: Td expected before Tj".into());
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

						lines.push(Line {
							start,
							end,
						})
					} else {
						return Err("While parsing pdf: m expected before l".into());
					}
				}
				_ => (),
			}
		}

		Ok(Self {
			lines,
			texts,
		})
	}

	fn extract_table_objects(&self) -> Vec<TableObjects> {
		//TODO get the regions of the tables, by finding the coordinates of the "Block" text and
		// use it to divide the lines on the page. Maybe even find the line below
		// '5:  15:15\n- 16:45' as the second divider

		let top_string_positions = self.texts.iter()
			.filter(|t| t.text == "Block")
			.map(|t| t.position)
			.collect::<Vec<Point>>();

		let bottom_string_positions = self.texts.iter()
			.filter("5:  15:15\n- 16:45")
			.map(|t| t.position)
			.collect::<Vec<Point>>();

		//TODO find every line that intersects with a line going from block and then choosing the closest

		todo!()
	}
}
