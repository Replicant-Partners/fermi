use askama::Template;

#[derive(Template)]
#[template(path = "test.html")]
struct TestTemplate {
    name: String,
}

fn main() {
    let tmpl = TestTemplate {
        name: "World".to_string(),
    };
    println!("{}", tmpl.render().unwrap());
}
