use lopdf::{Document, Stream};
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

    let mut stream = streams[1].clone();
    stream.decompress();
    let stream = stream.decode_content().unwrap();


    let x_borders;
    let y_borders;
    let texts;

    {
        let objects = extract_objects(&mut streams[1].clone());
        x_borders = objects.0.clone().drain(..).map(|l| l.0.x).collect::<Vec<i64>>();
        y_borders = objects.1.clone().drain(..).map(|l| l.0.y).collect::<Vec<i64>>();
        texts = objects.2;
    }

    //TODO put text into corresponding columns

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



    // println!("number of horizontal lines: {}", x_lines.len());
    // println!("number of vertical lines: {}", y_lines.len());
}

fn extract_objects(stream: &mut Stream) -> (Vec<Line>, Vec<Line>, Vec<Text>) {
    stream.decompress();
    let stream = stream.decode_content().unwrap();

    let mut texts = Vec::new();
    let mut lines = Vec::new();
    // let mut horizontal_lines = Vec::new();
    // let mut vertical_lines = Vec::new();


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

                    let position = Point::new((td_ops[0].as_f64().unwrap() as i64), (td_ops[1].as_f64().unwrap() as i64));

                    texts.push(Text::new(text, position))
                } else {
                    println!("{}", "ERROR: Td expected before Tj");
                }
            },
            "l" => {
                let m = &stream.operations[i - 1];

                if m.operator == "m" {
                    let m_ops = &m.operands;
                    let l_ops = &op.operands;

                    let start = Point::new((m_ops[0].as_f64().unwrap() as i64), (m_ops[1].as_f64().unwrap() as i64));

                    let end = Point::new((l_ops[0].as_f64().unwrap() as i64), (l_ops[1].as_f64().unwrap() as i64));

                    // let array = if (start.y - end.y as i64) == 0 {
                    //     &mut horizontal_lines
                    // } else if (start.x - end.x) == 0 {
                    //     &mut vertical_lines
                    // } else {
                    //     panic!("{}", "ERROR: line is diagonal")
                    // };
                    //
                    // array.push(Line::new(start, end));

                    lines.push(Line::new(start, end));
                } else {
                    println!("{}", "ERROR: m expected before l");
                }
            },
            _ => (),
        }
    }

    let mut x_lines = HashMap::new();
    let mut y_lines = HashMap::new();

    for line in lines {
        if line.1.y == line.0.y {
            x_lines.entry(line.0.y).and_modify(|y: &mut Vec<i64>| {
                y.push(line.0.x);
                y.push(line.1.x)
            }).or_insert(Vec::new());
        } else if line.1.x == line.0.x {
            y_lines.entry(line.0.x).and_modify(|y: &mut Vec<i64>| {
                y.push(line.0.y);
                y.push(line.1.y)
            }).or_insert(Vec::new());
        } else {
            panic!("{}", "ERROR: line is diagonal")
        };
    }

    let x_lines = x_lines.iter().map(|l| {
        let start = Point::new(*l.0, *l.1.iter().min().unwrap());
        let end = Point::new(*l.0, *l.1.iter().max().unwrap());

        Line::new(start, end)
    }).collect::<Vec<Line>>();

    let mut y_lines = y_lines.iter().map(|l| {
        let start = Point::new(*l.1.iter().min().unwrap(), *l.0);
        let end = Point::new(*l.1.iter().max().unwrap(), *l.0);

        Line::new(start, end)
    }).collect::<Vec<Line>>();

    //catch the wired sub divisions in the cells
    let y_lines = y_lines.drain(..)
        .filter(|l| (l.len() > ((l.len() / x_lines.len() as i64) + 100)))
        .collect::<Vec<Line>>();

    (x_lines, y_lines, texts)
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

#[derive(Debug)]
struct Line(Point, Point);

impl Line {
    fn new(p1: Point, p2: Point) -> Self {
        Self(p1, p2)
    }

    fn len(&self) -> i64 {
        (
            ((self.1.x - self.0.x) as f64).powi(2) +
            ((self.1.y - self.0.y) as f64).powi(2)
        ).sqrt() as i64
    }
}

#[derive(Eq, PartialEq, Hash, Clone, Debug)]
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