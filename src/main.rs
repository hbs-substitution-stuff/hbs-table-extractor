use lopdf::Document;
use encoding_rs::{WINDOWS_1252, Decoder, DecoderResult};
use std::error::Error;
use std::collections::HashMap;

fn main() {
    let pdf = Document::load("./VertretungsplanA4_Dienstag.pdf").unwrap();

    // let steams = pdf.catalog().unwrap().iter().filter_map(|e| match e.1 {
    // 	Object::Stream(s) => Some(s),
    // 	_ => None,
    // }).collect::<Vec<&Stream>>();
    //
    // println!("{}", steams.len());

    let mut streams = Vec::new();

    for page in pdf.page_iter() {
        for object_id in pdf.get_page_contents(page) {
            let object = pdf.get_object(object_id).unwrap();

            if let Ok(stream) = object.as_stream() {
                streams.push(stream.to_owned());
            }
        }
    }

    // for mut stream in streams {
    // 	stream.decompress();
    //
    // 	let content = stream.decode_content().unwrap();
    //
    // 	for operation in content.operations {
    // 		println!("{}", operation.operator);
    // 	}
    // 	println!("{}", "----------------------------------");
    // }

    let mut current = streams[1].clone();
    current.decompress();
    let current = current.decode_content().unwrap();

    // let date_bytes = current.operations
    //     .iter()
    //     .filter(|o| o.operator == "Tj")
    //     .nth(2)
    //     .unwrap()
    //     .operands[0]
    //     .as_str()
    //     .unwrap();
    //
    // let date = WINDOWS_1252.decode(date_bytes).0;

    let mut texts = Vec::new();
    let mut horizontal_lines = Vec::new();
    let mut vertical_lines = Vec::new();

    for (i, op) in current.operations.iter().enumerate() {
        match op.operator.as_str() {
            "Tj" => {
                let td = &current.operations[i - 1];

                if td.operator == "Td" {
                    let td_ops = &td.operands;
                    let tj_ops = &op.operands;

                    let text = WINDOWS_1252.decode(tj_ops[0].as_str().unwrap()).0
                        .parse()
                        .unwrap();

                    let position = Point::new((td_ops[0].as_f64().unwrap() as i64), (td_ops[1].as_f64().unwrap() as i64));

                    texts.push(Text::new(text, position))
                } else {
                    println!("{}", "ERROR: Td expected before Tj");
                }
            },
            "l" => {
                let m = &current.operations[i - 1];

                if m.operator == "m" {
                    let m_ops = &m.operands;
                    let l_ops = &op.operands;

                    let start = Point::new((m_ops[0].as_f64().unwrap() as i64), (m_ops[1].as_f64().unwrap() as i64));

                    let end = Point::new((l_ops[0].as_f64().unwrap() as i64), (l_ops[1].as_f64().unwrap() as i64));

                    let array = if (start.y - end.y as i64) == 0 {
                        &mut horizontal_lines
                    } else if (start.x - end.x) == 0 {
                        &mut vertical_lines
                    } else {
                        panic!("{}", "ERROR: line is diagonal")
                    };

                    array.push(Line::new(start, end));
                } else {
                    println!("{}", "ERROR: m expected before l");
                }
            },
            _ => (),
        }
    }

    let mut start_to_end = HashMap::new();

    for line in horizontal_lines {
        start_to_end.insert(line.0, line.1);
    }

    let mut to_remove = Vec::new();

    for start in start_to_end.keys() {
        let end = start_to_end.get(start).unwrap();
        if let Some(new_end) = start_to_end.get(end) {
            to_remove.push(end);

            let point = start_to_end.get_mut(start).unwrap();
            point.x = new_end.x;
            point.y = new_end.y;
        }
    }

    //TODO remove all with to_remove
}

struct Text {
    text: String,
    position: Point,
}

impl Text {
    fn new(text: String, position: Point) -> Self {
        Self {
            text,
            position,
        }
    }
}

struct Line(Point, Point);

impl Line {
    fn new(p1: Point, p2: Point) -> Self {
        Self(p1, p2)
    }

    fn set_end(&mut self, p1: Point) {
        self.1 = p1;
    }
}

#[derive(Eq, PartialEq, Hash, Clone)]
struct Point {
    x: i64,
    y: i64,
}

impl Point {
    fn new(x: i64, y: i64) -> Self {
        Self {
            x,
            y,
        }
    }
}