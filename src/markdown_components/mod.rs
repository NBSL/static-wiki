mod declarative;
mod table_of_contents;

use markdown_it::MarkdownIt;

type ComponentRegister = fn(&mut MarkdownIt);

const RUST_COMPONENTS: &[ComponentRegister] = &[register_table_of_contents];

pub(crate) fn add_rules(parser: &mut MarkdownIt) {
    add_rules_with_manifests(parser, &[]);
}

pub(crate) fn add_rules_with_manifests(parser: &mut MarkdownIt, manifests: &[String]) {
    for register in RUST_COMPONENTS {
        register(parser);
    }
    declarative::add_rules(parser, manifests);
}

fn register_table_of_contents(parser: &mut MarkdownIt) {
    parser.add_rule::<table_of_contents::TableOfContentsRule>();
}
