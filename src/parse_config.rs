use serde::Deserialize;

#[derive(Deserialize, Debug)]
pub struct Collection {
    pub git_base: Option<String>, // Optional global Git base URL
    #[serde(rename = "projects")]
    pub projects: Vec<ProjectInfo>,
}

impl Collection {
    pub fn validate(&self) -> anyhow::Result<()> {
        if !self.git_base.is_none() {
            return Ok(());
        }

        for p in self.projects.iter() {
            if let ProjectInfo::GitRel {
                git_rel: git,
                build_file: _,
                out_path: _,
            } = p
            {
                return Err(anyhow::anyhow!("the relative git `{git}` could no be resolved you need to set the `git_base` url"));
            }
        }

        return Ok(());
    }
}

#[derive(Debug)]
pub enum ProjectInfo {
    GitRel {
        git_rel: String,
        build_file: String,
        out_path: String,
    },
    GitUrl {
        git_url: String,
        build_file: String,
        out_path: String,
    },
    LocalPath {
        build_file: String,
        out_path: String,
    },
}

// Custom deserialization for Project enum
impl<'de> Deserialize<'de> for ProjectInfo {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct ProjectHelper {
            git_rel: Option<String>,
            git_url: Option<String>,
            build_file: String,
            out_path: String,
        }

        let helper = ProjectHelper::deserialize(deserializer)?;

        match (helper.git_rel, helper.git_url) {
            (Some(rel_git), None) => Ok(ProjectInfo::GitRel {
                git_rel: rel_git,
                build_file: helper.build_file,
                out_path: helper.out_path,
            }),
            (None, Some(git_url)) => Ok(ProjectInfo::GitUrl {
                git_url,
                build_file: helper.build_file,
                out_path: helper.out_path,
            }),
            (None, None) => Ok(ProjectInfo::LocalPath {
                build_file: helper.build_file,
                out_path: helper.out_path,
            }),
            (Some(_), Some(_)) => Err(serde::de::Error::custom(
                "A project can have either `git_rel` or `git_url`, but not both",
            )),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct Project {
    pub docker: String,
    pub exports: Vec<Export>,
}

#[derive(Debug, Deserialize)]
pub struct Export {
    pub path: String,
    pub name: String,
}
