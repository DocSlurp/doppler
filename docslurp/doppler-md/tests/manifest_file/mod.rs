//! Reads a folder's `manifest.jsonl` for the tests: checked line by line with the library's
//! reader (end line, counts, hash), then gathered into one object: the header's fields,
//! `images` and `batches` (each line without its `type`), and the end line's `complete`,
//! `resumable` and `totals`.

use std::path::Path;

use doppler_md::extract::manifest::{Kind, NAME, Reader};

pub fn manifest(dir: &Path) -> serde_json::Value {
    let f = std::fs::File::open(dir.join(NAME)).unwrap();
    let mut r = Reader::new(std::io::BufReader::new(f));
    let mut m = serde_json::Map::new();
    let (mut images, mut batches) = (Vec::new(), Vec::new());
    while let Some(line) = r.next().unwrap() {
        let mut o = match line.value {
            serde_json::Value::Object(o) => o,
            _ => unreachable!(),
        };
        o.remove("type");
        match line.kind {
            Kind::Header => m.extend(o),
            Kind::Image => images.push(serde_json::Value::Object(o)),
            Kind::Batch => batches.push(serde_json::Value::Object(o)),
            Kind::End => {
                for k in ["complete", "resumable", "totals"] {
                    m.insert(k.into(), o[k].clone());
                }
            }
        }
    }
    m.insert("images".into(), images.into());
    m.insert("batches".into(), batches.into());
    serde_json::Value::Object(m)
}
