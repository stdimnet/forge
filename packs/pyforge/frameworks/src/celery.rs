//! Celery tasks: functions decorated with `@shared_task`, `@app.task` or `@celery.task`
//! (with or without arguments); the task name is `name=` or `<module>.<function>`.

use serde::{Deserialize, Serialize};

use crate::source;
use crate::{Location, Project};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    /// The registered name: `name=` or `shop.tasks.send_receipt`.
    pub name: String,
    pub function: String,
    /// `shared_task` or `app.task` as written.
    pub decorator: String,
    /// `bind=True`.
    pub bound: bool,
    pub location: Location,
}

pub fn tasks(project: &Project) -> Vec<Task> {
    let mut out = Vec::new();
    for (path, lines) in project.python() {
        for (i, l) in lines.iter().enumerate() {
            let Some(("def", func)) = source::definition(l) else {
                continue;
            };
            for d in source::decorators(lines, i) {
                let t = &d.tokens;
                let open = t.iter().position(|x| x.is_op("(")).unwrap_or(t.len());
                let Some(callee) = source::dotted_exact(&t[1..open]) else {
                    continue;
                };
                let last = callee.rsplit('.').next().unwrap_or(&callee);
                let is_task = last == "shared_task" || (last == "task" && callee.contains('.'));
                if !is_task {
                    continue;
                }
                let args = (open < t.len())
                    .then(|| source::call_args(t, open).map(|(a, _)| a))
                    .flatten()
                    .unwrap_or_default();
                let name = source::keyword(&args, "name")
                    .and_then(|a| a.string())
                    .unwrap_or_else(|| format!("{}.{}", project.module_of(path), func.text));
                out.push(Task {
                    name,
                    function: func.text.clone(),
                    decorator: callee.clone(),
                    bound: source::keyword(&args, "bind").is_some_and(|a| a.is_true()),
                    location: project.locate(path, func.start),
                });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use crate::tests::project;

    #[test]
    fn task_decorators() {
        let p = project(&[
            ("requirements.txt", "celery\n"),
            ("proj/celery.py", "from celery import Celery\napp = Celery('proj')\n\n@app.task(bind=True)\ndef debug_task(self):\n    pass\n"),
            ("shop/tasks.py", "from celery import shared_task\n\n@shared_task\ndef send_receipt(order_id):\n    pass\n\n@shared_task(name='shop.cleanup', ignore_result=True)\ndef cleanup():\n    pass\n\n@other.decorator\ndef plain():\n    pass\n"),
        ]);
        let t: Vec<(&str, &str, bool)> = p
            .info
            .tasks
            .iter()
            .map(|t| (t.name.as_str(), t.decorator.as_str(), t.bound))
            .collect();
        assert_eq!(
            t,
            [
                ("proj.celery.debug_task", "app.task", true),
                ("shop.tasks.send_receipt", "shared_task", false),
                ("shop.cleanup", "shared_task", false),
            ]
        );
        assert_eq!(p.info.tasks[1].location.line, 3);
    }
}
