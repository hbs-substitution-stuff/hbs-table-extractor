use std::collections::HashSet;
use std::ffi::OsStr;
use std::path::Path;
use lopdf::{Document, Stream};
use std::error::Error;
use std::iter::FilterMap;
use std::slice::Iter;
use geo::{Line, Point};


/// the parser itself
pub struct PdfScheduleParser {
	document: Document,
	pages: Vec<PageObjects>,
}

/// all objects on a page
#[derive(Clone)]
pub struct PageObjects(Vec<TableObject>);

/// the text in the pdf
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
struct Text {
	text: String,
	position: Point<i64>,
}

impl Text {
	// only works left to right
	fn between_x(&self, limit_start: i64, limit_end: i64) -> bool {
		self.position.x() > limit_start && self.position.x() < limit_end
	}
}

/// all relevant pdf objects
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
enum TableObject {
	Line(Line<i64>),
	Text(Text),
}

impl TableObject {
	fn between_y(&self, limit_top: i64, limit_bottom: i64) -> bool {
		let between = |o| o < limit_top && o > limit_bottom;

		match self {
			Self::Text(t) => {
				between(t.position.y())
			},
			Self::Line(l) => {
				between(l.start.y) && between(l.end.y)
			},
		}
	}

	// only works left to right
	fn intersects_x_border(&self, border: i64) -> bool {
		match self {
			Self::Line(l) => if l.dy() == 0 {
				border >= l.start.x && border <= l.end.x
			} else { false },
			Self::Text(t) => t.position.y() == border,
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

	fn y(&self) -> i64 {
		match self {
			Self::Text(t) => t.position.y(),
			Self::Line(l) => if l.dy() == 0 { l.start.y } else { panic!("line is vertical") },
		}
	}
}

trait TableObjectIter {
	fn lines<'a>(&'a self) -> FilterMap<Iter<'_, TableObject>, fn(&'a TableObject) -> Option<&'a Line<i64>>>;

	fn texts<'a>(&'a self) -> FilterMap<Iter<'_, TableObject>, fn(&'a TableObject) -> Option<&'a Text>>;
}

impl TableObjectIter for PageObjects {
	fn lines<'a>(&'a self) -> FilterMap<Iter<'_, TableObject>, fn(&'a TableObject) -> Option<&'a Line<i64>>> {
		self.0.iter().filter_map(|o| if let TableObject::Line(l) = o {Some(l)} else {None})
	}

	fn texts<'a>(&'a self) -> FilterMap<Iter<'_, TableObject>, fn(&'a TableObject) -> Option<&'a Text>> {
		self.0.iter().filter_map(|o| if let TableObject::Text(t) = o {Some(t)} else {None})
	}
}

impl TableObjectIter for TableObjects {
	fn lines<'a>(&'a self) -> FilterMap<Iter<'_, TableObject>, fn(&'a TableObject) -> Option<&'a Line<i64>>> {
		self.0.iter().filter_map(|o| if let TableObject::Line(l) = o {Some(l)} else {None})
	}

	fn texts<'a>(&'a self) -> FilterMap<Iter<'_, TableObject>, fn(&'a TableObject) -> Option<&'a Text>> {
		self.0.iter().filter_map(|o| if let TableObject::Text(t) = o {Some(t)} else {None})
	}
}

impl TableObjectIter for TableColumn {
	fn lines<'a>(&'a self) -> FilterMap<Iter<'_, TableObject>, fn(&'a TableObject) -> Option<&'a Line<i64>>> {
		self.column.iter().filter_map(|o| if let TableObject::Line(l) = o {Some(l)} else {None})
	}

	fn texts<'a>(&'a self) -> FilterMap<Iter<'_, TableObject>, fn(&'a TableObject) -> Option<&'a Text>> {
		self.column.iter().filter_map(|o| if let TableObject::Text(t) = o {Some(t)} else {None})
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
					pages.push(PageObjects::from_stream(stream)?);
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
			.find(|t| t.text.contains("Datum: "))
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

	pub fn extract_tables(&self) -> Vec<Table> {
		self.pages.iter()
			.map(|p| p.extract_table_objects())
			.flatten()
			.map(|t: TableObjects| t.extract_columns())
			.map(|mut c| c.drain(..)
				.map(|mut t| t.generate_column())
				.collect()
			)
			.collect()
	}
}

type Table = Vec<Column>;
type Column = Vec<CellContent>;
type CellContent = Vec<String>;

impl PageObjects {
	fn from_stream(stream: &Stream) -> Result<Self, Box<dyn std::error::Error>> {
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

						let text = Document::decode_text(
							Some("WinAnsiEncoding"),
							tj_ops[0].as_str().unwrap()
						);

						let position = Point::new(
							td_ops[0].as_f64().unwrap() as i64,
							td_ops[1].as_f64().unwrap() as i64,
						);

						objects.push(TableObject::Text(Text {
							text,
							position
						}));
					} else {
						return Err("While parsing pdf: Td expected before Tj".into());
					}
				}
				"l" => {
					let m = &stream.operations[i - 1];

					if m.operator == "m" {
						let m_ops = &m.operands;
						let l_ops = &op.operands;

						let start = Point::new(
							m_ops[0].as_f64().unwrap() as i64,
							m_ops[1].as_f64().unwrap() as i64,
						);

						let end = Point::new(
							l_ops[0].as_f64().unwrap() as i64,
							l_ops[1].as_f64().unwrap() as i64,
						);

						objects.push(TableObject::Line(Line::new(start, end)))
					} else {
						return Err("While parsing pdf: m expected before l".into());
					}
				}
				_ => (),
			}
		}

		let mut result = HashSet::new();

		for object in objects {
			result.insert(object);
		}

		Ok(Self(result.drain().collect()))
	}

	fn extract_table_objects(&self) -> Vec<TableObjects> {
		let mut top_limits = self.texts()
			.filter(|t| t.text == "Block")
			.map(|t| t.position.y() + 4 /* add a tolerance of 4 */)
			.collect::<Vec<i64>>();

		top_limits.sort();

		let mut bottom_limits = self.texts()
			.filter(|t| t.text.contains("15:15"))
			.map(|t| t.position.y())
			.collect::<Vec<i64>>();

		bottom_limits.sort();

		// Sanity check
		if bottom_limits.len() != top_limits.len() {
			panic!("bottom and top limits don't match up");
		}

		// adjust bottom_limit to extend to the bottom line and add a tolerance
		let mut line_deltas = Vec::new();

		for limit in &bottom_limits {
			line_deltas.push(self.lines().filter_map(|l| {
				let delta = l.start.y - limit;

				if l.dy() == 0 && delta.is_negative() {
					Some(delta)
				} else {
					None
				}
			}).max().unwrap())
		}

		// Sanity check
		if line_deltas.len() != bottom_limits.len() {
			panic!("something went wrong")
		}

		let mut line_deltas = line_deltas.into_iter();

		let bottom_limit_y = bottom_limits.drain(..)
			.map(|l| l + line_deltas.next().unwrap() - 4 /* add a tolerance of -4 */)
			.collect::<Vec<i64>>();

		let mut extracted_tables = vec![TableObjects(Vec::new()); top_limits.len()];

		for object in &self.0 {
			for (idx, (top_bound, bottom_bound)) in top_limits.iter().zip(&bottom_limit_y).enumerate() {
				if object.between_y(*top_bound, *bottom_bound) {
					extracted_tables[idx].0.push(object.clone());
				}
			}
		}

		extracted_tables
	}
}

#[derive(Clone)]
struct TableObjects(Vec<TableObject>);

impl TableObjects {
	fn extract_columns(&self) -> Vec<TableColumn> {
		let header_height = self.texts()
			.find(|t| t.text == "Block")
			.expect("String 'Block' not found")
			.position.y();

		let mut columns = Vec::new();

		for header in self.texts() {
			// TODO merge with between_y function
			if header.position.y() < &header_height + 2 && /* 4 tolerance in total */
				header.position.y() > &header_height - 2 {
				if header.text != "Block" {
					columns.push(
						TableColumn {
							header: header.to_owned(),
							column: Vec::new(),
						}
					);
				}
			}
		}

		for i in 0..columns.len() {
			for object in &self.0 {
				if object.intersects_x_border(columns[i].header.position.x()) {
					columns[i].column.push(object.clone());
				}
			}
		}

		for i in 0..columns.len() {
			for text in self.texts() {
				if text.between_x(columns[i].start(), columns[i].end()) {
					columns[i].column.push(TableObject::Text(text.clone()));
				}
			}
		}

		columns
	}
}

struct TableColumn {
	header: Text,
	column: Vec<TableObject>,
}

impl TableColumn {
	fn generate_column(&mut self) -> Vec<Vec<String>> {
		// remove all vertical lines as they are not needed and interfere with the next steps
		self.column = self.column.drain(..).filter(|o| {
			!if let TableObject::Line(l) = o {
				l.dy() != 0
			} else {
				false
			}
		}).collect();

		//println!("{}, {}", self.header.text, self.header.position.x());

		// TODO remove the line segments before the extension line segments
		//self.column.sort_by(|l1, l2| l2.y().cmp(&l1.y()));

		let mut lines = self.lines().collect::<Vec<&Line<i64>>>();
		lines.sort_by(|l1, l2| l2.start.y.cmp(&l1.start.y));

		//println!("{}", lines.len());

		let texts = self.texts().map(|t| TableObject::Text(t.clone()));

		let mut offset = lines.iter();
		offset.next();

		let mut spacing = lines.iter()
			.zip(offset)
			.map(|(l, n)| {
				//println!("{}", l.start.y - n.end.y);
				l.start.y - n.start.y
			}).collect::<Vec<i64>>();

		//println!("{}", spacing.len());

		let smallest_space =  {
			let mut spacing_sorted = spacing.clone();
			spacing_sorted.sort();
			spacing_sorted.reverse();
			spacing_sorted.truncate(6);
			spacing_sorted[5]
		};

		// eiter put in front or at end
		spacing.push(smallest_space);

		let mut cleaned_column = lines.iter()
			.zip(spacing.iter())
			.filter(|(_, s)| *s >= &smallest_space)
			.map(|(l, _)| TableObject::Line(l.to_owned().to_owned()))
			.chain(texts)
			.collect::<Vec<TableObject>>();

		// sanity check
		//println!("{}", self.texts().count());
		if (cleaned_column.len() - self.texts().count()) != 7 {
			panic!("not exactly 7 lines");
		}

		cleaned_column.sort_by(|l1, l2| l2.y().cmp(&l1.y()));

		//println!("{:?}", cleaned_column);

		let mut result = vec![Vec::new(); 7];

		// sanity check
		if let TableObject::Line(_) = cleaned_column[0] {
			panic!("expected header text");
		}

		//cleaned_column.remove(0);

		let mut i = 0;

		for object in cleaned_column {
			//println!("i: {}", i as usize);
			match object {
				TableObject::Line(_) => i += 1,
				TableObject::Text(t) => result[i as usize].push(t.text),
			}
		}

		result
	}

	fn start(&self) -> i64 {
		self.lines().map(|l| l.start.x).min().expect("no lines in field 'column'")
	}

	fn end(&self) -> i64 {
		self.lines().map(|l| l.end.x).max().expect("no lines in field 'column'")
	}
}