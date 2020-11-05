use std::io::Seek;
use std::io::Write;

use rusty_peanuts_cli::xmp::get_metadata;

fn get_format(file: &std::fs::File) -> image::ImageFormat {
    let bufreader = std::io::BufReader::new(file);
    let reader = image::io::Reader::new(bufreader);
    let reader = reader
        .with_guessed_format()
        .expect("couldn't guess file format");

    let format = reader.format().expect("couldn't get guessed file format");

    format
}

#[async_std::main]
async fn main() -> std::io::Result<()> {
    dotenv::dotenv().ok();
    env_logger::init();

    let mut args = std::env::args();
    let path = args.nth(1).expect("missing argument");
    let path = std::path::Path::new(&path);

    let file_name = path
        .file_name()
        .expect("path does not point to a file")
        .to_string_lossy();
    let mut file = std::fs::File::open(path).expect("couldn't open file");

    match get_format(&file) {
        image::ImageFormat::Tiff => {
            file.seek(std::io::SeekFrom::Start(0))
                .expect("couldn't seek file to begining");
            let (xmp_xml, create_date, title, tags) = get_metadata(&file);

            log::info!("Create Date: {}", create_date);
            log::info!("Title: {:?}", title);
            log::info!("Tags: {:?}", tags);

            std::fs::File::create(&format!("xmp.{}.xml", file_name))
                .expect("could not create XMP metadata file")
                .write(xmp_xml.as_bytes())
                .expect("could not write XMP metadata to file");
        },
        format => {
            log::error!("Unupported format: {:?}", format);
            return Ok(());
        },
    }

    Ok(())
}
