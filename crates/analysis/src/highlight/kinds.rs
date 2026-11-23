//! The kinds of token that the colouring tells apart.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Plain,
    Comment,
    String,
    Number,
    Control,
    Keyword,
    Function,
    Type,
    Variable,
    Param,
    Builtin,
}

impl Kind {
    pub fn class(self) -> Option<&'static str> {
        match self {
            Kind::Plain => None,
            Kind::Comment => Some("token-comment"),
            Kind::String => Some("token-string"),
            Kind::Number => Some("token-number"),
            Kind::Control => Some("token-control"),
            Kind::Keyword => Some("token-keyword"),
            Kind::Function => Some("token-function"),
            Kind::Type => Some("token-type"),
            Kind::Variable => Some("token-variable"),
            Kind::Param => Some("token-parameter"),
            Kind::Builtin => Some("token-builtin"),
        }
    }
}
