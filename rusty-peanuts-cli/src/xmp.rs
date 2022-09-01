use quick_xml::de::from_str;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Alt {
    li: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct Title {
    #[serde(rename = "Alt")]
    alt: Alt,
}

#[derive(Debug, Deserialize)]
struct Bag {
    li: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct Subject {
    #[serde(rename = "Bag")]
    bag: Bag,
}

#[derive(Debug, Deserialize)]
struct Description {
    #[serde(rename = "xmp:CreateDate")]
    create_date: Option<String>,
    title: Option<Title>,
    subject: Option<Subject>,
}

#[derive(Debug, Deserialize)]
struct Rdf {
    #[serde(rename = "Description")]
    description: Vec<Description>,
}

#[derive(Debug, Deserialize)]
struct XmpMeta {
    #[serde(rename = "RDF")]
    rdf: Rdf,
}

pub fn get_metadata<R: std::io::Read + std::io::Seek>(
    read: R,
) -> (String, String, Option<String>, Vec<String>) {
    let bufreader = std::io::BufReader::new(read);
    let mut decoder = tiff::decoder::Decoder::new(bufreader).expect("couldn't make tiff decoder");

    let xmp_tag = tiff::tags::Tag::Unknown(700);
    let xmp_tag_data = decoder.get_tag(xmp_tag).expect("failed to get XMP tag");

    let xmp_bytes: Vec<_> = xmp_tag_data
        .into_u64_vec()
        .expect("coludn't convert XMP data into Vec<u64>")
        .into_iter()
        .map(|v| v as u8)
        .collect();
    let xmp_xml_data = String::from_utf8(xmp_bytes).expect("XMP tag had invalid UTF-8 data");

    let xmp_parsed: XmpMeta = from_str(&xmp_xml_data).expect("failed to parse XMP data");

    let (create_date, title, tags) = xmp_parsed
        .rdf
        .description
        .into_iter()
        .filter_map(|d| match (d.create_date, d.title, d.subject) {
            (Some(create_date), title_element, Some(subject)) => {
                let title = match title_element {
                    Some(t) => t.alt.li.into_iter().next(),
                    None => None,
                };
                let subject = subject.bag.li;
                Some((create_date, title, subject))
            },
            _ => None,
        })
        .next()
        .expect("couldn't find a single valid RDF.Description element in XMP metadata");

    (xmp_xml_data, create_date, title, tags)
}
