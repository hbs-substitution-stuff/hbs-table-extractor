mod pdf_schedule_parser;

use pdf_schedule_parser::PdfScheduleParser;

fn main() {
    let mut subs = PdfScheduleParser::new("./mutter_aller_scheiße.pdf").unwrap();
    //println!("{}", subs.extract_date().unwrap());

    println!("{:?}", subs.extract_tables());
    println!("{}", subs.extract_date().unwrap());
}