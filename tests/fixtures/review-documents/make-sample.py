#!/usr/bin/env python3
"""Create a fictional document used only in App Store screenshot captures."""
from pathlib import Path
from reportlab.pdfgen import canvas
from reportlab.lib.colors import HexColor

output = Path(__file__).with_name('membership-summary.pdf')
c = canvas.Canvas(str(output), pagesize=(612, 792), invariant=1, pageCompression=1)
c.setTitle('Sample membership summary')
c.setFillColor(HexColor('#2754aa'))
c.setFont('Helvetica-Bold', 13)
c.drawString(52, 742, 'NORTHSTAR COMMUNITY')
c.setFillColor(HexColor('#1c2737'))
c.setFont('Helvetica-Bold', 25)
c.drawString(52, 694, 'Membership summary')
c.setFont('Helvetica', 11)
c.setFillColor(HexColor('#536274'))
c.drawString(52, 671, 'Account details for your records')
c.setStrokeColor(HexColor('#d8dee7'))
c.line(52, 650, 560, 650)
for y, label, value in [(591, 'MEMBER NAME', 'Jordan Avery'), (530, 'MEMBER ID', 'NS-4829-1673')]:
    c.setFont('Helvetica-Bold', 11); c.setFillColor(HexColor('#536274')); c.drawString(52, y, label)
    c.setFont('Helvetica-Bold', 17); c.setFillColor(HexColor('#1c2737')); c.drawString(240, y, value)
    c.setStrokeColor(HexColor('#e4e8ee')); c.line(52, y-19, 560, y-19)
c.setFillColor(HexColor('#1c2737')); c.setFont('Helvetica-Bold', 15)
c.drawString(52, 453, 'Membership overview')
c.setFont('Helvetica', 12); c.setFillColor(HexColor('#425267'))
for y, line in [(423, 'Plan                         Community membership'), (396, 'Status                      Active'), (369, 'Access                     Member services and resources'),
                (310, 'Keep this summary for your records. You can review your'), (291, 'membership details whenever you need them.')]:
    c.drawString(52, y, line)
c.setStrokeColor(HexColor('#d8dee7')); c.line(52, 90, 560, 90)
c.setFillColor(HexColor('#758194')); c.setFont('Helvetica', 10)
c.drawString(52, 68, 'SAMPLE DOCUMENT  |  Fictional details')
c.drawRightString(560, 68, '1 / 1')
c.showPage(); c.save()
