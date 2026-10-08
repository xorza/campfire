use std::fmt::{self, Write};

use campfire_sim::Capability;

use crate::scripts::hook::Hook;
use crate::scripts::role_set::RoleSet;
use crate::scripts::script_api::ScriptApi;
use crate::scripts::script_api::api_owner::ApiOwner;
use crate::scripts::script_api::data_table::DataTable;
use crate::scripts::script_api::member_kind::MemberKind;
use crate::scripts::script_role::ScriptRole;

/// The opening of the generated reference.
const REFERENCE_HEAD: &str = "# Campfire — Script API reference

Generated from the script API's registry ([One source](08-script-api.md#one-source)); do not edit it. A test fails when it differs from what the registry writes; run that test with `CAMPFIRE_BLESS=1` to write it again. A name that runs is bound by the code that runs it; a planned one is one design 08 gives that the release does not run yet. The rules of the API are [design 08](08-script-api.md).
";

/// The script API's reference in Markdown, as its registry writes it: every name, hook, state
/// and data field, with whether the release runs it.
#[derive(Debug)]
pub(crate) struct ApiReference<'a>(pub(crate) &'a ScriptApi);

impl ApiReference<'_> {
    /// The reference's text.
    pub(crate) fn text(&self) -> String {
        let mut out = String::from(REFERENCE_HEAD);
        self.write(&mut out).expect("a string takes any text");
        out
    }

    fn write(&self, out: &mut String) -> fmt::Result {
        let roles = |roles: RoleSet| {
            if roles == RoleSet::ALL {
                "every role".to_owned()
            } else {
                let names: Vec<_> = roles.iter().map(ScriptRole::name).collect();
                names.join(", ")
            }
        };
        let capability =
            |capability: Option<Capability>| capability.map_or("core", Capability::name);
        for owner in ApiOwner::ALL {
            let members = self
                .0
                .members
                .iter()
                .filter(|member| member.spec.owner == owner);
            let ctx = owner == ApiOwner::Ctx;
            write!(out, "\n## {}\n\n", owner.title())?;
            out.push_str(if ctx {
                "| Name | Form | Roles | Capability | Status | What it is |\n| --- | --- | --- | --- | --- | --- |\n"
            } else {
                "| Name | Form | Capability | Status | What it is |\n| --- | --- | --- | --- | --- |\n"
            });
            for member in members {
                let spec = &member.spec;
                let form = match (spec.kind, member.writable) {
                    (MemberKind::Field | MemberKind::Value, true) => "written and read".to_owned(),
                    (MemberKind::Field | MemberKind::Value, false) => "read".to_owned(),
                    (MemberKind::Operator, _) => "operator".to_owned(),
                    (MemberKind::Call | MemberKind::Method, _) => {
                        let forms: Vec<_> = spec
                            .forms
                            .iter()
                            .map(|form| format!("`({})`", form.join(", ")))
                            .collect();
                        forms.join(" or ") + member.name_args_text().as_str()
                    }
                };
                let roles = if ctx {
                    format!(" {} |", roles(spec.roles))
                } else {
                    String::new()
                };
                writeln!(
                    out,
                    "| `{}` | {form} |{roles} {} | {} | {} |",
                    spec.name,
                    capability(spec.capability),
                    member.status,
                    spec.description,
                )?;
            }
        }
        out.push_str(
            "\n## Engine enums\n\nEach enum's module holds its members, and the function `named`, which gives the member a text names as data does; a member has `==`, `!=` and `to_string`, its name in data.\n\n| Enum | Members |\n| --- | --- |\n",
        );
        for record in &self.0.enums {
            let members: Vec<_> = record
                .members
                .iter()
                .map(|member| format!("`{member}`"))
                .collect();
            writeln!(out, "| `{}` | {} |", record.engine_enum, members.join(", "))?;
        }
        out.push_str(
            "\n## Hooks\n\n| Hook | Role | Capability | Status |\n| --- | --- | --- | --- |\n",
        );
        for status in Hook::ALL
            .iter()
            .filter_map(|&hook| self.0.hooks.iter().find(|status| status.hook == hook))
        {
            let hook = status.hook;
            writeln!(
                out,
                "| `{}({})` | {} | {} | {} |",
                hook.name(),
                hook.param_names().join(", "),
                hook.role().name(),
                capability(hook.capability()),
                status.status,
            )?;
        }
        out.push_str("\n## Tag properties\n\n| Property | Status |\n| --- | --- |\n");
        for status in &self.0.tag_properties {
            writeln!(out, "| `{}` | {} |", status.property, status.status)?;
        }
        out.push_str("\n## Data fields\n");
        for table in DataTable::ALL {
            writeln!(
                out,
                "\n### {}\n\n| Field | Status |\n| --- | --- |",
                table.title()
            )?;
            for field in self.0.data.iter().filter(|field| field.table == table) {
                writeln!(out, "| `{}` | {} |", field.name, field.status)?;
            }
        }
        Ok(())
    }
}
