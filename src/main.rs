use lopdf::{Document, Stream};
use encoding_rs::{WINDOWS_1252, Decoder, DecoderResult};
use std::error::Error;
use std::collections::{HashMap, HashSet};
use std::ffi::OsStr;
use std::fmt::{Display, Formatter};
use std::path::Path;
use std::process::Command;
use std::str;
use std::time::SystemTime;

use chrono::{Local, NaiveDate, Offset, Utc};
use serde::{Deserialize, Serialize};

/// One column with Substitutions from the PDF
#[derive(Serialize, Deserialize, PartialOrd, PartialEq, Debug)]
pub struct Substitutions {
    #[serde(rename(serialize = "0"))]
    #[serde(rename(deserialize = "0"))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block_0: Option<String>,
    #[serde(rename(serialize = "1"))]
    #[serde(rename(deserialize = "1"))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block_1: Option<String>,
    #[serde(rename(serialize = "2"))]
    #[serde(rename(deserialize = "2"))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block_2: Option<String>,
    #[serde(rename(serialize = "3"))]
    #[serde(rename(deserialize = "3"))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block_3: Option<String>,
    #[serde(rename(serialize = "4"))]
    #[serde(rename(deserialize = "4"))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block_4: Option<String>,
    #[serde(rename(serialize = "5"))]
    #[serde(rename(deserialize = "5"))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block_5: Option<String>,
}

impl Substitutions {
    pub fn new() -> Self {
        Self {
            block_0: None,
            block_1: None,
            block_2: None,
            block_3: None,
            block_4: None,
            block_5: None,
        }
    }
    pub fn first_substitution(&self) -> usize {
        self.as_array().iter().position(|b| b.is_some()).unwrap_or(0)
    }

    pub fn last_substitution(&self) -> usize {
        self.as_array().iter().rposition(|b| b.is_some()).unwrap_or(5)
    }

    pub fn as_array(&self) -> [&Option<String>; 6] {
        // One could consider also implementing Iterator
        [&self.block_0, &self.block_1, &self.block_2, &self.block_3, &self.block_4, &self.block_5]
    }
}

impl Display for Substitutions {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", serde_json::to_string_pretty(self).unwrap())
    }
}

/// Contains the extracted PDF data of the schedule PDF
#[derive(Serialize, Deserialize, Debug)]
pub struct SubstitutionSchedule {
    /// The creation date inside the PDF
    pub pdf_create_date: i64,
    /// The name of the class is the Key and the Value is a Substitutions struct
    entries: HashMap<String, Substitutions>,
    /// The time when the struct was created, used for comparing the age
    struct_time: u64,
}

impl SubstitutionSchedule {
    #[allow(clippy::ptr_arg)]
    fn table_to_substitutions(table: &Vec<Vec<String>>) -> HashMap<String, Substitutions> {
        let mut entries: HashMap<String, Substitutions> = HashMap::new();

        let classes = &table[0][1..];

        for class in classes {
            entries.insert(class.to_string(), Substitutions::new());
        }

        let mut row = 1;

        for lesson_idx in 0..5 {
            loop {
                for (i, substitution_part) in table[row][1..].iter().enumerate() {
                    let substitutions = entries.get_mut(&classes[i]).unwrap();

                    let block_option = match lesson_idx {
                        0 => &mut substitutions.block_0,
                        1 => &mut substitutions.block_1,
                        2 => &mut substitutions.block_2,
                        3 => &mut substitutions.block_3,
                        4 => &mut substitutions.block_4,
                        5 => &mut substitutions.block_5,
                        _ => panic!("more then 5 lessons used"),
                    };

                    if !substitution_part.is_empty() {
                        if let Some(block) = block_option {
                            block.push_str(&format!("\n{}", substitution_part.clone()));
                        } else {
                            let _ = block_option.insert(substitution_part.clone());
                        }
                    }
                }

                if table[row][0].starts_with('-') {
                    break;
                }
                row += 1;
            }

            row += 1;
        }

        entries
    }

    pub fn get_substitutions(&self, class: &str) -> Option<&Substitutions> {
        self.entries.get(class)
    }

    pub fn _get_entries(&self) -> &HashMap<String, Substitutions> { &self.entries }

    pub fn get_classes(&self) -> HashSet<String> {
        let mut classes = HashSet::new();

        for class in self.entries.keys() {
            classes.insert(class.clone());
        }

        classes
    }

    /// This function skips entries not present in the 'entries' `HashMap`
    #[allow(clippy::implicit_clone)]
    pub fn _get_entries_portion(&self, classes: &HashSet<&String>) -> HashMap<String, &Substitutions> {
        let mut portion = HashMap::new();

        for class in classes {
            if let Some(substitution) = self.entries.get(*class) {
                portion.insert(class.to_owned().to_owned(), substitution);
            }
        }

        portion
    }

    pub fn from_pdf_native<T: AsRef<Path> + AsRef<OsStr>>(path: T) -> Result<Self, Box<dyn std::error::Error>> {
        let pdf = Document::load(path).unwrap();

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


        let mut y_borders;
        let mut x_borders;
        let mut texts;

        {
            let mut table_objects = Self::extract_table_objects(&mut streams[0].clone())?;
            let mut objects = table_objects.remove(0);

            if objects.0.len() != 7 {
                panic!("horizontal lines should be exactly 7, got {}", objects.0.len())
            }

            y_borders = objects.0.drain(..).map(|l| l.0.x).collect::<Vec<i64>>();
            x_borders = objects.1.drain(..).map(|l| l.0.y).collect::<Vec<i64>>();
            texts = objects.2;
        }

        y_borders.sort();
        y_borders.reverse();
        x_borders.sort();

        // let mut columns = vec![vec![Vec::new(); x_borders.len()]; y_borders.len()];
        // //columns.repeat();
        //
        // let texts = texts[3..].to_vec();
        //
        // for text in &texts {
        //     for (x_index, x_border) in x_borders.iter().enumerate() {
        //         if text.position.y > *x_border {
        //             for (y_index, y_border) in y_borders.iter().enumerate() {
        //                 if text.position.x < *y_border {
        //                     columns[y_index][x_borders.len() - 1 - x_index].push(text);
        //                     break
        //                 }
        //             }
        //         }
        //     }
        // }

        println!("{:?}", y_borders);
        println!("{:?}", x_borders);

        let mut columns = vec![vec![Vec::new(); 7]; x_borders.len() - 1];

        for text in &texts {
            for (x_index, x_border) in x_borders.iter().enumerate() {
                if text.position.x < *x_border {
                    //columns[x_index][]
                    for (y_index, y_border) in y_borders.iter().enumerate() {
                        if text.position.y > *y_border {
                            columns[x_index - 1][y_index].push(&text.text);
                            break
                        }
                    }
                    break
                }
            }
        }

        println!("{:?}", columns);

        //let stringified = columns.drain(..).map(|mut v| v.drain(..).map(|mut v| v.drain(..).map(|s| s.text.to_owned()).collect::<String>()).collect::<Vec<String>>()).collect::<Vec<Vec<String>>>();

        //println!("{:?}", stringified);

        todo!()
        //TODO put text into corresponding columns
    }
    // -> [(y_lines, x_lines, texts)]
    fn extract_table_objects(stream: &mut Stream) -> Result<Vec<(Vec<Line>, Vec<Line>, Vec<Text>)>, Box<dyn std::error::Error>> {
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
                        return Err("While parsing pdf: Td expected before Tj".into());
                    }
                },
                "l" => {
                    let m = &stream.operations[i - 1];

                    if m.operator == "m" {
                        let m_ops = &m.operands;
                        let l_ops = &op.operands;

                        let start = Point::new((m_ops[0].as_f64().unwrap() as i64), (m_ops[1].as_f64().unwrap() as i64));
                        let end = Point::new((l_ops[0].as_f64().unwrap() as i64), (l_ops[1].as_f64().unwrap() as i64));

                        lines.push(Line::new(start, end));
                    } else {
                        return Err("While parsing pdf: m expected before l".into());
                    }
                },
                _ => (),
            }
        }

        let mut y_lines = HashMap::new();
        let mut x_lines = Vec::new();



        for line in lines.drain(..) {
            if line.1.y == line.0.y {
                y_lines.entry(line.0.y).and_modify(|y: &mut Vec<i64>| {
                    y.push(line.0.x);
                    y.push(line.1.x)
                }).or_insert(Vec::new());
            } else if line.1.x == line.0.x {
                x_lines.push(line)
            } else {
                return Err("While parsing pdf: line is diagonal".into())
            };
        };

        let mut y_lines = x_lines.iter().map(|l| {
            let start = Point::new(*l.1.iter().min().unwrap(), *l.0);
            let end = Point::new(*l.1.iter().max().unwrap(), *l.0);

            Line::new(start, end)
        }).collect::<Vec<Line>>();

        if y_lines.len() % 7 {
            panic!("horizontal lines should be a multiple of 7, got {}", objects.0.len())
        }

        y_lines.sort();

        let y_lines_tables = y_lines.chunks(7).collect::<Vec<Vec<Line>>>();

        for y_lines in y_lines_tables {
            y_lines..max() + 14;
            y_lines.min();
        }



        let x_lines = y_lines.iter().map(|l| {
            let start = Point::new(*l.0, *l.1.iter().min().unwrap());
            let end = Point::new(*l.0, *l.1.iter().max().unwrap());

            Line::new(start, end)
        }).collect::<Vec<Line>>();

        //catch the wired sub divisions in the cells
        let y_lines = y_lines.drain(..)
            .filter(|l| (l.len() > ((l.len() / x_lines.len() as i64) + 100)))
            .collect::<Vec<Line>>();

        Ok((x_lines, y_lines, texts))
    }
}

impl Display for SubstitutionSchedule {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", serde_json::to_string_pretty(self).unwrap())
    }
}

fn main() {
    let subs = SubstitutionSchedule::from_pdf_native("./VertretungsplanA4_Dienstag.pdf");

}

#[derive(Clone, Debug)]
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

#[derive(Debug, Clone)]
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

    fn between_horizontal_lines(&self, top: i64, bottom: i64) -> bool {
        self.0.y < top && self.1.y < top && self.0.y > bottom && self.1.y > bottom
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