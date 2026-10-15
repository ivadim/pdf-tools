//! Prints a page's decoded content stream, for debugging.
//!
//!     cargo run -p pdftools-core --example dump_page -- <file.pdf> [page] [max-chars]

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let path = args.next().ok_or("usage: dump_page <file.pdf> [page] [max-chars]")?;
    let page: u32 = args.next().map(|p| p.parse()).transpose()?.unwrap_or(1);
    let max: usize = args.next().map(|p| p.parse()).transpose()?.unwrap_or(2000);
    let doc = pdftools_core::load(std::path::Path::new(&path))?;
    let id = *doc.get_pages().get(&page).ok_or("no such page")?;
    let content = doc.get_page_content(id)?;
    let text = String::from_utf8_lossy(&content);
    println!("{}", text.chars().take(max).collect::<String>());
    Ok(())
}
