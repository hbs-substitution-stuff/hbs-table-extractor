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
		let date_idx_start = pdf.find("Datum: ").ok_or("date not found")?;
		let date_idx_end = pdf[date_idx_start..].find('\n').ok_or("date end not found")? + date_idx_start;
    
    //TODO chrono::NaiveDateTime::parse_from_str(s: &str, fmt: &str)

		let date_str: Vec<u32> = pdf[date_idx_start..date_idx_end].split(", ")
			.last()
			.ok_or("date string has no ','")?
			.split('.')
			.collect::<Vec<&str>>()
			.iter()
			.map(|s| (*s).parse::<u32>().unwrap())
			.collect();

		#[allow(clippy::cast_possible_wrap)]
		Ok(chrono::Date::<Local>::from_utc(
			NaiveDate::from_ymd(date_str[2] as i32, date_str[1], date_str[0]),
			Utc.fix(),
		).and_hms(0, 0, 0).timestamp())
	}
}

type CellContent<'a> = Vec<&'a str>;
type ColumnarTable<'a> = Vec<Vec<CellContent<'a>>>;

impl TableObjects {
	fn new(objects: PageStream) -> Result<Self, Box<dyn Error>> {
		let mut vertical_borders = Vec::new();
		let mut horizontal_borders = Vec::new();

		for line in objects.lines {
		    if line.start.y == line.end.y {
		        horizontal_borders.push(line.start.y)
		    } else if line.start.x == line.end.x {
		        vertical_borders.push(line.start.y)
		    } else {
		        return Err("While parsing pdf: line is diagonal".into())
		    };
		};

		//TODO get rid of the random short dividers within some cells. Possibly by grouping them by
		// length and removing the groups which don't add to a multiple of 7 and by checking if they
		// are on top of one of another.

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
						let text = WINDOWS_1252.decode(tj_ops[0].as_str().unwrap()).0
							.parse()
							.unwrap();

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

	fn extract_table_objects(&self) -> Vec<(Vec<Line>, Vec<Text>)> {
		//TODO get the regions of the tables, by finding the coordinates of the "Block" text and
		// use it to divide the lines on the page. Maybe even find the line below
		// '5:  15:15\n- 16:45' as the second divider
		todo!()
	}
}
