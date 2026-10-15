# Test fixtures

`pdfjs/` holds real-world PDFs from Mozilla's pdf.js test suite
(<https://github.com/mozilla/pdf.js/tree/master/test/pdfs>, Apache-2.0 repository).
They're used by the integration tests (`crates/pdftools-core/tests/`) and the
UI tests (`crates/pdftools-app/tests/`). Each was picked for something the tools must cope with:

| File | Why |
| --- | --- |
| tracemonkey.pdf | 14-page paper; embedded Type1 fonts, lots of text |
| TAMReview.pdf | Multi-page article with TrueType fonts |
| mixedfonts.pdf | Many font types on one page |
| arial_unicode_ab_cidfont.pdf | CID (Type0) font |
| annotation-text-widget.pdf | Interactive form fields |
| annotation-highlight.pdf | Annotations with appearance streams |
| alphatrans.pdf | Transparency groups, soft masks |
| cmykjpeg.pdf | CMYK JPEG image |
| calrgb.pdf | CalRGB colour space, images |
| devicen.pdf | DeviceN (spot) colours |
| canvas.pdf | Vector drawing with RGB colours |
| asciihexdecode.pdf | ASCIIHex-encoded content stream |
| rotation.pdf | Pages with `/Rotate` |
| sizes.pdf | Pages of different sizes |
| issue5010.pdf | Small file with unusual structure |

To refresh a file: download it again from the pdf.js repository at the same path.

`encrypted/` holds `sizes.pdf` encrypted by [qpdf](https://qpdf.readthedocs.io)
(through pikepdf), so Unlock is tested against another implementation than
the one Protect uses:

| File | Encryption |
| --- | --- |
| aes256-password-test.pdf | AES-256 (R6), open password `test` |
| aes256-restrictions-only.pdf | AES-256 (R6), no open password; printing and copying restricted |
