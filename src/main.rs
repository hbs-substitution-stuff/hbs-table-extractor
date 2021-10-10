use lopdf::Document;
use encoding_rs::{WINDOWS_1252, Decoder, DecoderResult};
use std::error::Error;

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

    let date_bytes = current.operations
        .iter()
        .filter(|o| o.operator == "Tj")
        .nth(2)
        .unwrap()
        .operands[0]
        .as_str()
        .unwrap();

    let date = WINDOWS_1252.decode(date_bytes).0;

    let mut objects = Vec::new();

    for (i, op) in current.operations.iter().enumerate() {
        if op.operator == "Tj" {
            let text = WINDOWS_1252.decode(op.operands[0].as_str().unwrap()).0;
            let td = &current.operations[i - 1].operands;

            let position = Point::new(td[0].as_f64().unwrap(), td[1].as_f64().unwrap());

            objects.push(Objects::Text(Text::new(text.parse().unwrap(), position)))
        }

        if op.operator == "l" {
            let m = &current.operations[i - 1].operands;
            let start = Point::new(m[0].as_f64().unwrap(), m[1].as_f64().unwrap());

            let l = &op.operands;
            let end = Point::new(l[0].as_f64().unwrap(), l[1].as_f64().unwrap());

            objects.push(Objects::Line(Line::new(start, end)));
        }
    }

    // for ob in objects {
    // 	match ob {
    // 		Objects::Text(t) => println!("Text: {}, ({}, {})", t.text, t.position.x, t.position.y),
    // 		Objects::Line(l) => {
    // 			if ((l.0.x as i64) - (l.1.x as i64)) == 0 {
    // 				println!("Vertical Line: ({}, {}), ({}, {})", l.0.x, l.0.y, l.1.x, l.1.y);
    // 			} else if ((l.0.y as i64) - (l.1.y as i64)) == 0 {
    // 				println!("Horizontal Line: ({}, {}), ({}, {})", l.0.x, l.0.y, l.1.x, l.1.y);
    // 			} else {
    // 				println!("Diagonal Line: ({}, {}), ({}, {})", l.0.x, l.0.y, l.1.x, l.1.y);
    // 			}
    // 		},
    // 	}
    // }

    print!("{}", "x = [");
    for ob in &objects {
        match ob {
            Objects::Text(t) => {
                print!("{}, ", t.position.x)
            },
            Objects::Line(l) => {
                print!("{}, ", l.0.x)
            },
        }
    }
    print!("{}", "]");

    println!("{}", "");

    print!("{}", "y = [");
    for ob in &objects {
        match ob {
            Objects::Text(t) => {
                print!("{}, ", t.position.y)
            },
            Objects::Line(l) => {
                print!("{}, ", l.0.y)
            },
        }
    }
    print!("{}", "]");

    println!("{}", "");

    print!("{}", "u = [");
    for ob in &objects {
        match ob {
            Objects::Text(t) => {
                print!("{}, ", 1)
            },
            Objects::Line(l) => {
                print!("{}, ", l.1.x - l.0.x)
            },
        }
    }
    print!("{}", "]");

    println!("{}", "");

    print!("{}", "v = [");
    for ob in &objects {
        match ob {
            Objects::Text(t) => {
                print!("{}, ", 1)
            },
            Objects::Line(l) => {
                print!("{}, ", l.1.y - l.0.y)
            },
        }
    }
    print!("{}", "]");
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
}

struct Point {
    x: f64,
    y: f64,
}

impl Point {
    fn new(x: f64, y: f64) -> Self {
        Self {
            x,
            y,
        }
    }
}

enum Objects {
    Text(Text),
    Line(Line),
}