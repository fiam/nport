use std::borrow::Cow;

use axum::http::StatusCode;
use axum::response::{Html, IntoResponse};
use serde::Serialize;
use tera::{Context, Tera};

use crate::server::Result;

pub struct Templates {
    tera: Tera,
}

impl Templates {
    #[cfg(dev)]
    pub fn new() -> Result<Self> {
        let tera = Tera::new("templates/**/*.html")?;
        Ok(Self { tera })
    }

    #[cfg(not(dev))]
    pub fn new() -> Result<Self> {
        let mut tera = Tera::default();

        let base = include_str!("../../templates/base.html");
        tera.add_raw_template("base.html", base)?;

        let index = include_str!("../../templates/index.html");
        tera.add_raw_template("index.html", index)?;

        let error = include_str!("../../templates/error.html");
        tera.add_raw_template("error.html", error)?;

        let stats = include_str!("../../templates/stats.html");
        tera.add_raw_template("stats.html", stats)?;

        Ok(Self { tera })
    }

    pub fn renderer<S: AsRef<str>>(&self, template_name: S) -> Renderer {
        Renderer {
            templates: self,
            template_name: template_name.as_ref().to_string(),
            context: None,
            status_code: None,
        }
    }

    pub fn render<S: AsRef<str>>(
        &self,
        template_name: S,
        context: &Context,
    ) -> Result<Html<String>> {
        self.reload()?;
        let result = self.tera.render(template_name.as_ref(), context)?;
        Ok(result.into())
    }

    #[cfg(dev)]
    fn reload(&self) -> Result<()> {
        let ptr = &self.tera as *const Tera;
        let mut_ptr = ptr as *mut Tera;
        unsafe {
            let tera = &mut *mut_ptr;
            tera.full_reload()?;
        }
        Ok(())
    }

    #[cfg(not(dev))]
    fn reload(&self) -> Result<()> {
        Ok(())
    }
}

pub struct Renderer<'a> {
    templates: &'a Templates,
    template_name: String,
    status_code: Option<StatusCode>,
    context: Option<Context>,
}

impl<'a> Renderer<'a> {
    pub fn with_status(&mut self, status_code: StatusCode) -> &mut Self {
        self.status_code = Some(status_code);
        self
    }

    pub fn with_context(&mut self, context: Context) -> &mut Self {
        self.context = Some(context);
        self
    }

    pub fn with_data<S: Serialize>(&mut self, data: S) -> &mut Self {
        self.context = Some(Context::from_serialize(data).unwrap());
        self
    }

    pub fn render_response(&self) -> Result<impl IntoResponse> {
        let context = self
            .context
            .as_ref()
            .map(Cow::Borrowed)
            .unwrap_or_else(|| Cow::Owned(Context::new()));
        let html = self.templates.render(&self.template_name, &context)?;
        let status_code = self.status_code.unwrap_or(StatusCode::OK);
        Ok((status_code, html))
    }
}
