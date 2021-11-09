use std::ffi::OsStr;
use std::path::Path;
use encoding_rs::WINDOWS_1252;
use lopdf::{Document, Stream};

struct PdfParser {
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

struct PdfTable {
	vertical_lines: Vec<Border>,
	horizontal_border: Vec<Border>,
	text: Vec<Text>,
}

impl PdfParser {
	fn new<T: AsRef<Path> + AsRef<OsStr>>(path: T) -> Result<Self, Box<dyn std::error::Error>> {
		let mut pdf = Document::load(path).unwrap();

		let mut page_streams = Vec::new();

		for page in pdf.page_iter() {
			for object_id in pdf.get_page_contents(page) {
				let object = pdf.get_object_mut(object_id).unwrap();

				if let Ok(stream) = object.as_stream_mut() {
					page_streams.push(PageStream::new(stream));
				};
			};
		};

		let mut stream = streams[1].clone();
		stream.decompress();
		let stream = stream.decode_content().unwrap();
	}
}

impl PageStream {
	fn new(stream: &mut Stream) -> Result<Self, Box<dyn std::error::Error>> {
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
}