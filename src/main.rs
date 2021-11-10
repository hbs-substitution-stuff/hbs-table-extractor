mod pdf_schedule_parser;

use pdf_schedule_parser::PdfScheduleParser;

fn main() {
    let subs = PdfScheduleParser::new("./VertretungsplanA4_Dienstag.pdf");

}