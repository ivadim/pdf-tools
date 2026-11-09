import AppKit
import ImageIO

let repo = URL(fileURLWithPath: CommandLine.arguments[1])
let out = URL(fileURLWithPath: CommandLine.arguments[2])
struct Shot { let source: String; let file: String; let lines: [String]; let caption: String }
let shots = [
 Shot(source: "home.png", file: "01-on-device", lines: ["Your PDFs.", "Your device."], caption: "Useful tools, right where your files are."),
 Shot(source: "tools/merge-configured.png", file: "02-merge", lines: ["Bring pages", "together."], caption: "Merge PDFs and arrange pages your way."),
 Shot(source: "tools/compress-configured.png", file: "03-compress", lines: ["Make room", "to share."], caption: "Choose the compression that fits."),
 Shot(source: "tools/redact-configured.png", file: "04-redact", lines: ["Keep private", "details private."], caption: "Mark areas and create a redacted copy."),
 Shot(source: "tools/images-configured.png", file: "05-images", lines: ["Photos in.", "PDF out."], caption: "Choose your page size and margins.")
]
func color(_ r: CGFloat,_ g: CGFloat,_ b: CGFloat) -> NSColor { NSColor(srgbRed:r/255, green:g/255, blue:b/255, alpha:1) }
func image(_ path: String) -> NSImage { guard let im = NSImage(contentsOf: repo.appendingPathComponent(path)) else { fatalError("Missing \(path)") }; return im }
func text(_ s: String,_ x: CGFloat,_ y: CGFloat,_ size: CGFloat,_ weight: NSFont.Weight,_ c: NSColor) { (s as NSString).draw(at:NSPoint(x:x,y:y), withAttributes:[.font:NSFont.systemFont(ofSize:size,weight:weight),.foregroundColor:c]) }
func box(_ rect:NSRect,_ radius:CGFloat,_ fill:NSColor) { fill.setFill(); NSBezierPath(roundedRect:rect,xRadius:radius,yRadius:radius).fill() }
func canvas(_ w:Int,_ h:Int,_ render:() -> Void) -> NSBitmapImageRep {
 let bitmap = NSBitmapImageRep(bitmapDataPlanes:nil,pixelsWide:w,pixelsHigh:h,bitsPerSample:8,samplesPerPixel:4,hasAlpha:true,isPlanar:false,colorSpaceName:.deviceRGB,bytesPerRow:0,bitsPerPixel:0)!
 let ctx = NSGraphicsContext(bitmapImageRep:bitmap)!
 NSGraphicsContext.saveGraphicsState(); NSGraphicsContext.current = ctx; ctx.imageInterpolation = .high; render(); NSGraphicsContext.restoreGraphicsState(); return bitmap
}
func write(_ bitmap:NSBitmapImageRep,_ name:String,_ png:Bool = false) throws {
 let u = out.appendingPathComponent(name); try FileManager.default.createDirectory(at:u.deletingLastPathComponent(),withIntermediateDirectories:true)
 let data = bitmap.representation(using:png ? .png : .jpeg,properties:png ? [:] : [.compressionFactor:0.94])!; try data.write(to:u)
 print(name)
}
for mode in ["light", "dark"] {
 let dark = mode == "dark"
 for (i,s) in shots.enumerated() {
  let b = canvas(1206,2622) {
   let bg = dark ? color(17,23,37) : color(239,243,249)
   let ink = dark ? NSColor.white : color(25,33,48)
   bg.setFill(); NSRect(x:0,y:0,width:1206,height:2622).fill()
   color(68,111,224).setFill(); NSRect(x:76,y:2498,width:58,height:8).fill()
   text("PDF TOOLS  /  \(String(format:"%02d",i+1))",155,2477,26,.semibold,dark ? color(161,187,255) : color(49,91,190))
   text(s.lines[0],76,2340,95,.bold,ink); text(s.lines[1],76,2233,95,.bold,ink)
   text(s.caption,80,2170,32,.medium,dark ? color(188,198,218) : color(76,88,107))
   let rect = NSRect(x:153,y:55,width:900,height:900*2622/1206)
   let border = rect.insetBy(dx:-5,dy:-5)
   box(border,42,dark ? color(67,79,101) : color(207,216,230))
   NSGraphicsContext.saveGraphicsState(); NSBezierPath(roundedRect:rect,xRadius:37,yRadius:37).addClip()
   image("docs/design-reviews/screenshots/ios/\(mode)/\(s.source)").draw(in:rect)
   NSGraphicsContext.restoreGraphicsState()
  }
  try write(b,"listing/\(mode)/\(s.file).jpg")
 }
}
// Universal 16:9 creative: centered message and genuine app UI, with generous margins.
let creative = canvas(5244,2950) {
 color(18,26,42).setFill(); NSRect(x:0,y:0,width:5244,height:2950).fill()
 box(NSRect(x:1140,y:730,width:2964,height:1490),80,color(28,39,59))
 color(95,139,255).setFill(); NSRect(x:1390,y:2035,width:94,height:12).fill()
 text("PDF TOOLS",1510,2009,62,.bold,color(180,199,249))
 text("Your PDFs.",1385,1710,188,.bold,NSColor.white)
 text("Your device.",1385,1490,188,.bold,NSColor.white)
 text("Merge, compress, redact and more.",1395,1360,66,.medium,color(193,205,225))
 let rect = NSRect(x:3290,y:820,width:530,height:1152)
 box(rect.insetBy(dx:-8,dy:-8),35,color(83,99,129))
 NSGraphicsContext.saveGraphicsState(); NSBezierPath(roundedRect:rect,xRadius:28,yRadius:28).addClip(); image("docs/design-reviews/screenshots/ios/dark/tools/merge-configured.png").draw(in:rect); NSGraphicsContext.restoreGraphicsState()
}
try write(creative,"creative/universal-header-search.jpg")
