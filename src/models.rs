use serde_derive::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub(crate) enum TargetDir {
    #[serde(rename = "functions")]
    Functions,
    #[serde(rename = "completions")]
    Completions,
    #[serde(rename = "conf.d")]
    ConfD,
    #[serde(rename = "themes")]
    Themes,
}

impl TargetDir {
    pub(crate) fn as_str(&self) -> &str {
        match self {
            TargetDir::Functions => "functions",
            TargetDir::Completions => "completions",
            TargetDir::ConfD => "conf.d",
            TargetDir::Themes => "themes",
        }
    }
    pub(crate) fn all() -> Vec<Self> {
        vec![
            TargetDir::Functions,
            TargetDir::Completions,
            TargetDir::ConfD,
            TargetDir::Themes,
        ]
    }
}

impl std::str::FromStr for TargetDir {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "functions" => Ok(TargetDir::Functions),
            "completions" => Ok(TargetDir::Completions),
            "conf.d" => Ok(TargetDir::ConfD),
            "themes" => Ok(TargetDir::Themes),
            _ => Err(format!("Invalid target dir: {}", s)),
        }
    }
}
