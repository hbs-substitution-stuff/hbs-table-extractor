mod pdf_schedule_parser;

use pdf_schedule_parser::PdfScheduleParser;

fn main() {
    let mut subs = PdfScheduleParser::new("./mutter_aller_scheiße.pdf").unwrap();
    //println!("{}", subs.extract_date().unwrap());

    let mut a = subs.pages[0].extract_table_objects();

    let b = &mut a[2];
    b.generate_table();
}