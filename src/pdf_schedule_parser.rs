use std::ffi::OsStr;
use std::path::Path;
use lopdf::{Document, Stream};
use std::error::Error;
use std::iter::FilterMap;
use std::slice::Iter;
use geo::{Line, Point};
use chrono::NaiveDate;


/// the parser itself
pub struct PdfScheduleParser {
	document: Document,
	pages: Vec<PageObjects>,
}

/// all objects on a page
#[derive(Clone)]
pub struct PageObjects(Vec<TableObject>);

/// the text in the pdf
#[derive(Clone, Debug)]
struct Text {
	text: String,
	position: Point<T>,
}

impl Text {
	// only works left to right
	fn between_x(&self, limit_start: i64, limit_end: i64) -> bool {
		self.position.x() > limit_start && self.position.x() < limit_end
	}
}

/// all relevant pdf objects
#[derive(Clone, Debug)]
enum TableObject {
	Line(Line<T>),
	Text(Text),
}

impl TableObject {
	fn between_y(&self, limit_top: i64, limit_bottom: i64) -> bool {
		let between = |o| o < limit_top && o > limit_bottom;

		match self {
			Self::Text(t) => {
				between(t.position.y)
			},
			Self::Line(l) => {
				between(l.start.y) && between(l.end.y)
			},
		}
	}

	// only works left to right
	fn intersects_y_border(&self, border: i64) -> bool {
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
			Self::Text(t) => t.position.x(),
			Self::Line(l) => if l.dy() == 0 { l.start.y } else { panic!("line is vertical") },
		}
	}
}

trait TableObjectIter {
	fn lines<'a>(&'a self) -> FilterMap<Iter<'_, TableObject>, fn(&'a TableObject) -> Option<&'a Line<T>>> {
		self.0.iter().filter_map(|o| if let TableObject::Line(l) = o {Some(l)} else {None})
	}

	fn texts<'a>(&'a self) -> FilterMap<Iter<'_, TableObject>, fn(&'a TableObject) -> Option<&'a Text>> {
		self.0.iter().filter_map(|o| if let TableObject::Text(t) = o {Some(t)} else {None})
	}
}

impl TableObjectIter for PageObjects {}

impl TableObjectIter for TableObjects {}

impl TableObjectIter for TableColumn {
	fn lines<'a>(&'a self) -> FilterMap<Iter<'_, TableObject>, fn(&'a TableObject) -> Option<&'a Line<T>>> {
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

	pub fn extract_tables() -> Vec<Table> {
		todo!()
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
							x: l_ops[0].as_f64().unwrap() as i64,
							y: l_ops[1].as_f64().unwrap() as i64,
						);

						objects.push(TableObject::Line(Line::new(start, end)))
					} else {
						return Err("While parsing pdf: m expected before l".into());
					}
				}
				_ => (),
			}
		}

		Ok(Self(objects))
	}

	fn extract_table_objects(&self) -> Vec<TableObjects> {
		let mut top_limits = self.texts()
			.filter(|t| t.text == "Block")
			.map(|t| t.position.y + 4 /* add a tolerance of 4 */)
			.collect::<Vec<i64>>();

		top_limits.sort();

		let mut bottom_limits = self.texts()
			.filter(|t| t.text.contains("15:15"))
			.map(|t| t.position.y)
			.collect::<Vec<i64>>();

		bottom_limits.sort();

		// Sanity check
		if bottom_limits.len() != top_limits.len() {
			panic!("bottom and top limits don't match up");
		}

		// adjust bottom_limit to extend to the bottom line and add a tolerance
		let mut line_deltas = Vec::new();

		for limit in &bottom_limits {
			line_deltas.push(self.lines().filter_map(|line| {
				let delta = line.start.y - limit;

				if line.horizontal() && delta.is_negative() {
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

		for object in self.0 {
			for (idx, (top_bound, bottom_bound)) in top_limits.iter().zip(&bottom_limit_y).enumerate() {
				if object.between_y(*top_bound, *bottom_bound) {
					extracted_tables[idx].0.push(object.clone());
				}
			}
		}

		extracted_tables
	}
}

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
			if header.position.y() < header_height + 2 && /* 4 tolerance in total */
				header.position.y() > limit_bottom - 2 {
				columns.push(
					TableColumn {
						header: header.clone(),
						column: Vec::new(),
					}
				);
			}
		}

		for mut column in columns {
			for object in self.0 {
				if object.intersects_y_border(column.header.position.y()) {
					column.column.push(object);
				}
			}
		}

		for mut column in columns {
			for text in self.texts() {
				if text.between_x(column.start(), column.end()) {
					column.column.push(TableObject::Text(text.clone()));
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
			!if let TableObject::Line(o) = l {
				o.dy() != 0
			} else {
				false
			}
		}).collect();

		// TODO remove the line segments before the extension line segments
		//self.column.sort_by(|l1, l2| l2.y().cmp(&l1.y()));

		let mut lines = self.lines().collect::<Vec<&Line<i64>>>();
		lines.sort_by(|l1, l2| l2.start.y.cmp(&l1.start.y));

		let texts = self.texts().map(|t| TableObject::Text(t.clone()));

		let mut offset = lines.iter();
		offset.next();

		let spacing = lines.iter()
			.zip(offset)
			.map(|(l, n)| {
				println!("should be positive (if not reverse sorting): {}", line.start.y - next.start.y);
				line.start.y - next.start.y
			}).collect::<Vec<i64>>();

		let smallest_space =  {
			let mut spacing_sorted = spacing.clone();
			spacing_sorted.sort();
			spacing_sorted.reverse();
			spacing_sorted.truncate(7);
			spacing_sorted[6]
		};


		

		let cleaned_column = lines.iter()
			.zip(spacing.iter())
			.filter(|(_, s)| s > &&smallest_space)
			.map(|(l, _)| TableObject::Line(l.to_owned().to_owned()))
			.chain(texts)
			.collect::<Vec<TableObject>>();

		// TODO generate column with the rest through sorting

		todo!()
	}

	fn start(&self) -> i64 {
		self.lines().map(|l| l.start.x).min().expect("no lines in field 'column'")
	}

	fn end(&self) -> i64 {
		self.lines().map(|l| l.end.x).max().expect("no lines in field 'column'")
	}
}