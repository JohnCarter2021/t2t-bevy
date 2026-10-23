use sim::script::{Catalog, Program, SourceOrigin, SourceResolver};

struct One(String);
impl SourceResolver for One {
    fn read(&self, module: &str) -> Result<String, String> {
        if module == "iw4l_br/zone" {
            Ok(self.0.clone())
        } else {
            Err(format!("missing {module}"))
        }
    }
    fn read_bytes(&self, module: &str) -> Result<Vec<u8>, String> {
        self.read(module).map(|s| s.into_bytes())
    }
    fn origin(&self, _module: &str) -> SourceOrigin {
        SourceOrigin::BuiltIn
    }
}

fn main() {
    let src = std::fs::read_to_string(std::env::args().nth(1).expect("path"))
        .expect("read gsc");
    let resolver = One(src);
    match Program::load(&resolver, &["iw4l_br/zone"], &Catalog::iw4()) {
        Ok(_) => println!("GSC COMPILE OK"),
        Err(e) => {
            println!("GSC COMPILE FAILED: {e:?}");
            std::process::exit(1);
        }
    }
}
