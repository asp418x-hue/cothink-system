//! Memory benchmark: builds a large in-interpreter data set and holds it
//! alive, so peak RSS captures the interpreter's per-object cost.
//!
//! Usage:
//! ```text
//! cargo run --release --example membench            # 30k map entries
//! CH3=100000 KIND=class cargo run --release --example membench
//! ```
//!
//! `CH3` selects the entry count (default 30_000); `KIND=class` builds
//! `Chapter` class instances instead of map literals, so `InstanceData`
//! field storage is what gets exercised. Watch peak RSS (e.g. `\time -v`).
use d4rt_rs::Context;

fn main() {
    let items: usize = std::env::var("CH3").map(|s| s.parse().unwrap()).unwrap_or(30_000);
    let use_class = std::env::var("KIND").map(|s| s == "class").unwrap_or(false);
    let src = if use_class {
        format!(
            r#"
class Chapter {{
  final int id;
  final String title;
  final String url;
  final String desc;
  final String lang;
  final List meta;
  Chapter(this.id, this.title, this.url, this.desc, this.lang)
      : meta = [0, 1, 2];
}}
void main() {{
  final n = {items};
  final chunk = 'Lorem ipsum dolor sit amet consectetur adipiscing elit sed do eiusmod';
  final list = [];
  for (var i = 0; i < n; i++) {{
    list.add(Chapter(i, 'Chapter ' + i.toString(),
        'https://api.akuma.moe/manga/1609/chapter/' + i.toString() + '/?x=1',
        chunk + i.toString(), 'en-US'));
  }}
  print(list.length);
}}
"#,
            items = items
        )
    } else {
        format!(
            r#"
void main() {{
  final n = {items};
  final chunk = 'Lorem ipsum dolor sit amet consectetur adipiscing elit sed do eiusmod';
  final list = [];
  for (var i = 0; i < n; i++) {{
    final chapter = {{
      'id': i,
      'title': 'Chapter ' + i.toString(),
      'url': 'https://api.akuma.moe/manga/1609/chapter/' + i.toString() + '/?x=1',
      'desc': chunk + i.toString(),
      'lang': 'en-US',
    }};
    final meta = [];
    for (var j = 0; j < 3; j++) meta.add(j);
    chapter['meta'] = meta;
    list.add(chapter);
  }}
  print(list.length);
}}
"#,
            items = items
        )
    };
    let mut ctx = Context::new();
    let v = ctx.execute(&src, "main", vec![], Default::default()).unwrap();
    std::mem::forget(v);
    println!("done");
}