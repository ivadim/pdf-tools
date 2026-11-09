# Screenshot document

`membership-summary.pdf` is a one-page fictional membership summary made for
the Redact marketing capture. Its member name, ID, organization and plan are
invented. The screenshot test marks the two values while retaining their field
labels. It is an input document, not a redacted output.

`make-sample.py` regenerates it using ReportLab from the bundled workspace
Python runtime. Changes to the page layout also require updating the normalized
redaction coordinates in `markSampleDetails` and recapturing all three devices.
