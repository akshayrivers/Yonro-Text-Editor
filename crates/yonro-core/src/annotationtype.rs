#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum AnnotationType {
    Char,
    LifetimeSpecifier,
    // Search/UI
    Match,
    SelectedMatch,

    // Generic lexical
    Number,
    String,
    Comment,

    // Programming languages
    Keyword,
    Type,
    KnownValue,

    // Markdown / Text
    Heading,
    Emphasis,
    InlineCode,
    CodeBlock,
    Link,
    ListItem,

    // Optional (useful for plain text / misc)
    Identifier,
}
