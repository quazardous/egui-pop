---
egui-pop-api: minor
---

Help > Suggest an idea (`help.suggest`) sits beside Report a problem when the
application says where issues go: an idea opens a new issue titled "Idea for
<page>" with the page alone, in the idea form named by
`AboutPlugin::idea_form("feature.yml")`, or in the body without one
(`about::idea_url`, `Report::idea_url`). Report a problem keeps the bug form.
