use quick_xml::Reader;
use quick_xml::events::Event;

fn parse_rating(value: &str) -> Result<i8, String> {
    let rating = value
        .trim()
        .parse::<i8>()
        .map_err(|_| "XMP Rating 不是整数".to_string())?;
    if !(-1..=5).contains(&rating) {
        return Err("XMP Rating 必须在 -1 到 5 之间".to_string());
    }
    Ok(rating)
}

pub(crate) fn xmp_rating(input: &[u8]) -> Result<Option<i8>, String> {
    let mut reader = Reader::from_reader(input);
    reader.config_mut().trim_text(true);
    let mut inside_rating = false;
    let mut rating = None;

    loop {
        match reader.read_event() {
            Ok(Event::Start(element)) => {
                for attribute in element.attributes() {
                    let attribute = attribute.map_err(|error| format!("XMP 属性无效：{error}"))?;
                    if attribute.key.local_name().as_ref() == b"Rating" {
                        let value = attribute
                            .decode_and_unescape_value(reader.decoder())
                            .map_err(|error| format!("无法读取 XMP Rating：{error}"))?;
                        if rating.replace(parse_rating(&value)?).is_some() {
                            return Err("XMP 中包含多个 Rating".to_string());
                        }
                    }
                }
                inside_rating = element.local_name().as_ref() == b"Rating";
            }
            Ok(Event::Empty(element)) => {
                for attribute in element.attributes() {
                    let attribute = attribute.map_err(|error| format!("XMP 属性无效：{error}"))?;
                    if attribute.key.local_name().as_ref() == b"Rating" {
                        let value = attribute
                            .decode_and_unescape_value(reader.decoder())
                            .map_err(|error| format!("无法读取 XMP Rating：{error}"))?;
                        if rating.replace(parse_rating(&value)?).is_some() {
                            return Err("XMP 中包含多个 Rating".to_string());
                        }
                    }
                }
            }
            Ok(Event::Text(text)) if inside_rating => {
                let value = text
                    .decode()
                    .map_err(|error| format!("无法解码 XMP Rating：{error}"))?;
                if rating.replace(parse_rating(&value)?).is_some() {
                    return Err("XMP 中包含多个 Rating".to_string());
                }
            }
            Ok(Event::End(element)) if element.local_name().as_ref() == b"Rating" => {
                inside_rating = false;
            }
            Ok(Event::Eof) => return Ok(rating),
            Ok(_) => {}
            Err(error) => return Err(format!("XMP XML 无效：{error}")),
        }
    }
}
