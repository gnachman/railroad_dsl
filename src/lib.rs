#[macro_use]
extern crate pest_derive;
use railroad as rr;

use pest::iterators::Pair;
use pest::Parser;
use railroad::svg;

use std::ffi::{CStr, CString};
use std::os::raw::c_char;

#[derive(Parser)]
#[grammar = "parser.pest"]
struct RRParser;

pub struct Diagram {
    pub width: i64,
    pub height: i64,
    pub diagram: rr::Diagram<Box<dyn rr::Node>>,
}

fn unescape(pair: &Pair<'_, Rule>) -> String {
    let s = pair.as_str();
    let mut result = String::with_capacity(s.len());
    let mut iter = s[1..s.len() - 1].chars();
    while let Some(ch) = iter.next() {
        result.push(match ch {
            '\\' => iter.next().expect("no escaped char?"),
            _ => ch,
        });
    }
    result
}

fn binary<F, T>(pair: Pair<'_, Rule>, f: F) -> Box<dyn rr::Node>
where
    T: rr::Node + 'static,
    F: FnOnce(Box<dyn rr::Node>, Pair<'_, Rule>) -> T,
{
    let mut inner = pair.into_inner();
    let node = make_node(inner.next().expect("pair cannot be empty"));
    if let Some(pair) = inner.next() {
        Box::new(f(node, pair))
    } else {
        node
    }
}

fn make_node(pair: Pair<'_, Rule>) -> Box<dyn rr::Node> {
    use Rule::*;
    match pair.as_rule() {
        term => Box::new(rr::Terminal::new(unescape(&pair))),
        nonterm => Box::new(rr::NonTerminal::new(unescape(&pair))),
        comment => Box::new(rr::Comment::new(unescape(&pair))),
        empty => Box::new(rr::Empty),
        sequence => Box::new(rr::Sequence::new(
            pair.into_inner().map(make_node).collect(),
        )),
        stack => Box::new(rr::Stack::new(pair.into_inner().map(make_node).collect())),
        choice => Box::new(rr::Choice::new(pair.into_inner().map(make_node).collect())),
        opt_expr => binary(pair, |node, _| rr::Optional::new(node)),
        rpt_expr => binary(pair, |first, second| {
            rr::Repeat::new(first, make_node(second))
        }),
        lbox_expr => binary(pair, |first, second| {
            rr::LabeledBox::new(first, make_node(second))
        }),
        _ => unreachable!(),
    }
}

fn start_to_end(root: Box<dyn rr::Node>) -> Box<dyn rr::Node> {
    Box::new(rr::Sequence::new(vec![
        Box::new(rr::SimpleStart) as Box<dyn rr::Node>,
        root,
        Box::new(rr::SimpleEnd),
    ]))
}

pub fn compile(src: &str, css: &str) -> Result<Diagram, Box<pest::error::Error<Rule>>> {
    let mut result = RRParser::parse(Rule::input, src)?;
    let trees = result.next().expect("expected root_expr").into_inner();
    let mut trees: Vec<_> = trees.map(|p| start_to_end(make_node(p))).collect();
    let root = if trees.len() == 1 {
        trees.remove(0)
    } else {
        Box::new(rr::VerticalGrid::new(trees))
    };

    let mut diagram = rr::Diagram::new(root);
    diagram.add_element(
        svg::Element::new("style")
            .set("type", "text/css")
            .raw_text(css),
    );

    let width = (&diagram as &dyn rr::Node).width();
    let height = (&diagram as &dyn rr::Node).height();
    Ok(Diagram {
        width,
        height,
        diagram,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use railroad::DEFAULT_CSS;
    use std::env;
    use std::fs;
    use std::io::Read;
    use std::path;

    #[test]
    fn examples_must_parse() {
        let home = env::var_os("CARGO_MANIFEST_DIR").unwrap();
        let mut exmpl_dir = path::PathBuf::from(home);
        exmpl_dir.push("examples");
        for path in fs::read_dir(exmpl_dir).unwrap().filter_map(Result::ok) {
            if let Some(filename) = path.file_name().to_str() {
                if filename.ends_with("diagram.txt") {
                    eprintln!("Compiling `{filename}`");
                    let mut buffer = String::new();
                    fs::File::open(path.path())
                        .unwrap()
                        .read_to_string(&mut buffer)
                        .unwrap();
                    if let Err(e) = compile(&buffer, DEFAULT_CSS) {
                        panic!("Failed to compile {}", e.with_path(filename));
                    }
                }
            }
        }
    }
}

#[no_mangle]
pub extern "C" fn railroad_dsl_to_svg(
    input: *const c_char,
    css:   *const c_char,
) -> *mut c_char {
    // Safety: caller must pass valid, NUL-terminated pointers
    assert!(!input.is_null());
    assert!(!css.is_null());

    // DSL source -> &str
    let c_input = unsafe { CStr::from_ptr(input) };
    let src     = c_input.to_str().expect("invalid UTF-8 in DSL source");

    // CSS source -> &str
    let c_css = unsafe { CStr::from_ptr(css) };
    let css_str = c_css.to_str().expect("invalid UTF-8 in CSS");

    // Compile DSL and render
    let diagram = compile(src, css_str)
        .expect("compile failed")
        .diagram; 
    let svg = diagram.to_string();

    // Return a C string -> caller must free
    let c_string = CString::new(svg).expect("NUL byte in SVG data");
    c_string.into_raw()
}

/// Free any string allocated by Rust (e.g. from dsl_to_svg)
#[no_mangle]
pub extern "C" fn railroad_string_free(s: *mut c_char) {
    if s.is_null() {
        return;
    }
    unsafe {
        drop(CString::from_raw(s));
    }
}
