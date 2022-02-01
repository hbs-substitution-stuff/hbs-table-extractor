use std::collections::{HashMap, HashSet};
use std::ffi::OsStr;
use std::path::Path;
use lopdf::{Document, Stream};
use std::error::Error;
use std::fs::OpenOptions;
use std::io::Read;
use std::iter::FilterMap;
use std::slice::Iter;
use geo::{Line, Point};
use substitution_common::{SubstitutionColumn, SubstitutionPDFExtractor, SubstitutionSchedule};


/// the parser itself
pub struct HbsTableExtractor(Vec<PageObjects>);

/// all objects on a page
#[derive(Clone)]
struct PageObjects(Vec<TableObject>);

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

	fn y(&self) -> Result<i64, Box<dyn Error>> {
		match self {
			Self::Text(t) => Ok(t.position.y()),
			Self::Line(l) => if l.dy() == 0 { Ok(l.start.y) } else { Err("line is vertical".into()) },
		}
	}
}

impl HbsTableExtractor {
	pub fn new<T: AsRef<Path> + AsRef<OsStr>>(path: T) -> Result<Self, Box<dyn Error>> {
		Self::load_from(OpenOptions::new().read(true).open(path)?)
	}

	pub fn load_from<R: Read>(src: R) -> Result<Self, Box<dyn Error>> {
		let document = Document::load_from(src)?;

		let mut pages = Vec::new();

		for page in document.page_iter() {
			for object_id in document.get_page_contents(page) {
				let object = document.get_object(object_id)?;

				if let Ok(stream) = object.as_stream() {
					pages.push(PageObjects::from_stream(stream)?);
				};
			};
		};

		Ok(Self(pages))
	}

	pub fn extract_date(&self) -> Result<i64, Box<dyn Error>> {
		let date_string = self.0.iter()
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

	// flattens by the first two vectors and joins the most inner one with '\n'
	pub fn extract_tables_simple(&mut self) -> Result<Vec<Vec<String>>, Box<dyn Error>> {
		let result = self.extract_tables()?;
		Ok(result.into_iter().flatten()
			.flatten()
			.map(|co| {
				co.iter()
					.map(|c| {
						c.into_iter()
							.fold(String::new(), |a, b| a + b + "\n")
					})
					.collect::<Vec<String>>()
			})
			.collect())
	}

	pub fn extract_tables(&mut self) -> Result<Vec<Page>, Box<dyn Error>> {
		Ok(self.0.iter()
			.map(|p| p.extract_table_objects())
			.collect::<Result<Vec<Vec<TableObjects>>, Box<dyn Error>>>()?
			.iter()
			.map(|tc| tc.iter().map(|t| t.extract_columns()))
			.map(|c|
				c.map(|mut tt| tt.drain(..)
					.map(|mut t|
						t.generate_column()
					).collect::<Result<Vec<Vec<Vec<String>>>, Box<dyn Error>>>()
				).collect::<Result<Vec<Vec<Vec<Vec<String>>>>, Box<dyn Error>>>()
			).collect::<Result<Vec<Page>, Box<dyn Error>>>()?)
	}
}

type Page = Vec<Table>;
type Table = Vec<Column>;
type Column = Vec<CellContent>;
type CellContent = Vec<String>;

impl PageObjects {
	fn from_stream(stream: &Stream) -> Result<Self, Box<dyn std::error::Error>> {
		let mut stream = stream.to_owned();
		stream.decompress();
		let stream = stream.decode_content()?;

		let mut objects = HashSet::new();


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
							tj_ops[0].as_str()?
						);

						let position = Point::new(
							td_ops[0].as_f64()? as i64,
							td_ops[1].as_f64()? as i64,
						);

						objects.insert(TableObject::Text(Text {
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
							m_ops[0].as_f64()? as i64,
							m_ops[1].as_f64()? as i64,
						);

						let end = Point::new(
							l_ops[0].as_f64()? as i64,
							l_ops[1].as_f64()? as i64,
						);

						objects.insert(TableObject::Line(Line::new(start, end)));
					} else {
						return Err("While parsing pdf: m expected before l".into());
					}
				}
				_ => (),
			}
		}

		Ok(Self(objects.drain().collect()))
	}

	fn extract_table_objects(&self) -> Result<Vec<TableObjects>, Box<dyn Error>> {
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
			return Err("bottom and top limits don't match up".into())
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
			}).max().ok_or("table bound could not be found")?)
		}

		// // Sanity check
		// if line_deltas.len() != bottom_limits.len() {
		// 	return Err("something went wrong".into())
		// }

		let mut line_deltas = line_deltas.into_iter();

		let bottom_limit_y = bottom_limits.drain(..)
			.map(|l| line_deltas.next().map(|d| l + d - 4 /* add a tolerance of -4 */))
			.collect::<Option<Vec<i64>>>().ok_or("line_deltas has a different length than bottom_limits")?;

		let mut extracted_tables = vec![TableObjects(Vec::new()); top_limits.len()];

		for object in &self.0 {
			for (idx, (top_bound, bottom_bound)) in top_limits.iter().zip(&bottom_limit_y).enumerate() {
				if object.between_y(*top_bound, *bottom_bound) {
					extracted_tables[idx].0.push(object.clone());
				}
			}
		}

		Ok(extracted_tables)
	}

	fn lines<'a>(&'a self) -> FilterMap<Iter<'_, TableObject>, fn(&'a TableObject) -> Option<&'a Line<i64>>> {
		self.0.iter().filter_map(|o| if let TableObject::Line(l) = o {Some(l)} else {None})
	}

	fn texts<'a>(&'a self) -> FilterMap<Iter<'_, TableObject>, fn(&'a TableObject) -> Option<&'a Text>> {
		self.0.iter().filter_map(|o| if let TableObject::Text(t) = o {Some(t)} else {None})
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

	// fn _lines<'a>(&'a self) -> FilterMap<Iter<'_, TableObject>, fn(&'a TableObject) -> Option<&'a Line<i64>>> {
	// 	self.0.iter().filter_map(|o| if let TableObject::Line(l) = o {Some(l)} else {None})
	// }

	fn texts<'a>(&'a self) -> FilterMap<Iter<'_, TableObject>, fn(&'a TableObject) -> Option<&'a Text>> {
		self.0.iter().filter_map(|o| if let TableObject::Text(t) = o {Some(t)} else {None})
	}
}

struct TableColumn {
	header: Text,
	column: Vec<TableObject>,
}

impl TableColumn {
	fn generate_column(&mut self) -> Result<Vec<Vec<String>>, Box<dyn Error>> {
		// remove all vertical lines as they are not needed and interfere with the next steps
		self.column = self.column.drain(..).filter(|o| {
			!if let TableObject::Line(l) = o {
				l.dy() != 0
			} else {
				false
			}
		}).collect();

		let mut lines = self.lines().collect::<Vec<&Line<i64>>>();
		lines.sort_by(|l1, l2| l2.start.y.cmp(&l1.start.y));


		let texts = self.texts().map(|t| TableObject::Text(t.clone()));

		let mut offset = lines.iter();
		offset.next();

		let mut spacing = lines.iter()
			.zip(offset)
			.map(|(l, n)| {
				l.start.y - n.start.y
			}).collect::<Vec<i64>>();

		let smallest_space =  {
			let mut spacing_sorted = spacing.clone();
			spacing_sorted.sort();
			spacing_sorted.reverse();
			spacing_sorted.truncate(6);
			spacing_sorted[5]
		};

		spacing.push(smallest_space);

		let mut cleaned_column = lines.iter()
			.zip(spacing.iter())
			.filter(|(_, s)| *s >= &smallest_space)
			.map(|(l, _)| TableObject::Line(l.to_owned().to_owned()))
			.chain(texts)
			.collect::<Vec<TableObject>>();

		// sanity check
		if (cleaned_column.len() - self.texts().count()) != 7 {
			return Err("not exactly 7 lines".into())
		}

		// don't remove this, needed in combination with the sort by
		if cleaned_column.iter()
			.fold(false, |_, t| if let TableObject::Line(l) = t { l.dy() != 0} else { false }) {
			return Err("vertical line in vector".into())
		}

		cleaned_column.sort_by(|l1, l2| l2.y().unwrap().cmp(&l1.y().unwrap()));

		let mut result = vec![Vec::new(); 7];

		// sanity check
		if let TableObject::Line(_) = cleaned_column[0] {
			return Err("expected header text".into())
		}

		let mut i = 0;

		for object in cleaned_column {
			match object {
				TableObject::Line(_) => i += 1,
				TableObject::Text(t) => result[i as usize].push(t.text),
			}
		}

		Ok(result)
	}

	fn start(&self) -> i64 {
		self.lines().map(|l| l.start.x).min().expect("no lines in field 'column'")
	}

	fn end(&self) -> i64 {
		self.lines().map(|l| l.end.x).max().expect("no lines in field 'column'")
	}

	fn lines<'a>(&'a self) -> FilterMap<Iter<'_, TableObject>, fn(&'a TableObject) -> Option<&'a Line<i64>>> {
		self.column.iter().filter_map(|o| if let TableObject::Line(l) = o {Some(l)} else {None})
	}

	fn texts<'a>(&'a self) -> FilterMap<Iter<'_, TableObject>, fn(&'a TableObject) -> Option<&'a Text>> {
		self.column.iter().filter_map(|o| if let TableObject::Text(t) = o {Some(t)} else {None})
	}
}

impl SubstitutionPDFExtractor for HbsTableExtractor {
	fn schedule_from_pdf<R: Read>(pdf: R) -> Result<SubstitutionSchedule, Box<dyn Error>> {
		let mut extractor = HbsTableExtractor::load_from(pdf)?;
		let mut entries = HashMap::new();

		for column in extractor.extract_tables()?.iter().flatten().flatten() {
			entries.insert(
				column[0][0].clone(),
				SubstitutionColumn::from_2d_vec(column[..6].to_vec())?
			);
		}

		Ok(SubstitutionSchedule {
			pdf_issue_date: extractor.extract_date()?,
			entries,
		})
	}
}