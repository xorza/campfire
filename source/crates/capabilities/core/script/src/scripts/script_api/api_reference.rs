use std::fmt::{self, Write};

use campfire_sim::Capability;

use crate::scripts::api_holder::ApiHolder;
use crate::scripts::data_table::DataTable;
use crate::scripts::given_names::NameShape;
use crate::scripts::member_kind::MemberKind;
use crate::scripts::planned_capability::PlannedCapability;
use crate::scripts::role_set::RoleSet;
use crate::scripts::script_api::ScriptApi;
use crate::scripts::script_role::ScriptRole;

/// The opening of the generated reference.
const REFERENCE_HEAD: &str = "# Campfire — Script API reference

Generated from the script API's registry ([One source](06-script-api.md#one-source)); do not edit it. A test fails when it differs from what the registry writes; run that test with `CAMPFIRE_BLESS=1` to write it again. A name that runs is bound by the code that runs it; a planned one is one the design gives that the release does not run yet. The rules of the API are in [Script API](06-script-api.md).
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
            } else if roles == RoleSet::SIM {
                "every role of the sim".to_owned()
            } else {
                let names: Vec<_> = roles.iter().map(ScriptRole::name).collect();
                names.join(", ")
            }
        };
        let capability = ApiReference::capability_name;
        let mut holders: Vec<_> = self
            .0
            .members
            .iter()
            .map(|member| member.decl.holder)
            .collect();
        holders.dedup();
        for holder in holders {
            let members = self
                .0
                .members
                .iter()
                .filter(|member| member.decl.holder == holder);
            let ctx = holder == ApiHolder::CTX;
            write!(out, "\n## {}\n\n", holder.title())?;
            out.push_str(if ctx {
                "| Name | Form | Roles | Capability | Status | What it is |\n| --- | --- | --- | --- | --- | --- |\n"
            } else {
                "| Name | Form | Capability | Status | What it is |\n| --- | --- | --- | --- | --- |\n"
            });
            for member in members {
                let spec = &member.decl;
                let mut form = match (spec.kind, member.writable) {
                    (MemberKind::Field | MemberKind::Value, true) => "written and read".to_owned(),
                    (MemberKind::Field | MemberKind::Value, false) => "read".to_owned(),
                    (MemberKind::Operator, _) => "operator".to_owned(),
                    (MemberKind::Call | MemberKind::Method, _) => {
                        let typed = |(at, name): (usize, &&str)| member.typed_arg(at, name);
                        let forms: Vec<_> = spec
                            .forms
                            .iter()
                            .map(|form| {
                                let args: Vec<_> = form.iter().enumerate().map(typed).collect();
                                format!("`({})`", args.join(", "))
                            })
                            .collect();
                        forms.join(" or ") + member.name_args_text().as_str()
                    }
                };
                if let Some(names) = member.output.and_then(|output| output.names) {
                    let kind = names.kind;
                    match names.shape {
                        NameShape::One => write!(form, ", gives {} {kind}", kind.article())?,
                        NameShape::List => write!(form, ", gives a list of {kind} names")?,
                    }
                }
                let roles = if ctx && spec.own_script {
                    format!(" {}'s own script |", roles(spec.roles))
                } else if ctx {
                    format!(" {} |", roles(spec.roles))
                } else {
                    String::new()
                };
                writeln!(
                    out,
                    "| `{}` | {form} |{roles} {} | {} | {} |",
                    spec.name,
                    capability(spec.capability, spec.planned_in),
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
        let mut hooks: Vec<_> = self.0.hooks.iter().collect();
        hooks.sort_by_key(|status| (status.hook.role, status.hook.name));
        for status in hooks {
            let hook = status.hook;
            writeln!(
                out,
                "| `{}({})` | {} | {} | {} |",
                hook.name,
                hook.param_names.join(", "),
                hook.role.name(),
                capability(status.capability, status.planned_in),
                status.status,
            )?;
        }
        out.push_str("\n## Tag properties\n\n| Property | Status |\n| --- | --- |\n");
        for status in &self.0.tag_properties {
            writeln!(out, "| `{}` | {} |", status.property, status.status)?;
        }
        self.write_data(out)
    }

    /// The data fields, table by table.
    fn write_data(&self, out: &mut String) -> fmt::Result {
        out.push_str("\n## Data fields\n");
        let mut tables: Vec<DataTable> = Vec::new();
        for field in &self.0.data {
            if !tables.contains(&field.table) {
                tables.push(field.table);
            }
        }
        for table in tables {
            writeln!(
                out,
                "\n### {}\n\n| Field | Capability | Status |\n| --- | --- | --- |",
                table.title()
            )?;
            for field in self.0.data.iter().filter(|field| field.table == table) {
                writeln!(
                    out,
                    "| `{}` | {} | {} |",
                    field.name,
                    ApiReference::capability_name(field.capability, None),
                    field.status
                )?;
            }
        }
        Ok(())
    }

    /// The name of `planned`, a capability the release does not build, else of `capability`,
    /// or `core` for none.
    const fn capability_name(
        capability: Option<Capability>,
        planned: Option<PlannedCapability>,
    ) -> &'static str {
        match (planned, capability) {
            (Some(planned), _) => planned.name(),
            (None, Some(capability)) => capability.name(),
            (None, None) => "core",
        }
    }
}
