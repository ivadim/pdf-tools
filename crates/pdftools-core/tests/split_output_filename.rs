use pdftools_core::split_output_filename;

#[test]
fn pads_only_when_part_count_has_two_digits() {
    assert_eq!(split_output_filename("report", 1, 9), "report-1.pdf");
    assert_eq!(split_output_filename("report", 12, 12), "report-12.pdf");
    assert_eq!(split_output_filename("report", 2, 12), "report-02.pdf");
}
