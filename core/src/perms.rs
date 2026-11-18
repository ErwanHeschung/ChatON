//! Community permissions (spec/protocol.md § 9): the Rust twin of
//! `server/internal/community/perms.go`, held to `spec/test-vectors/permissions.json`.
//! Keys are lowercase hex here, as in the tags.

use serde::{Deserialize, Serialize};

/// In spec order: one bit each.
pub const PERMISSIONS: [&str; 21] = [
    "administrator",
    "manage_community",
    "manage_channels",
    "manage_roles",
    "manage_invites",
    "create_invite",
    "kick_members",
    "ban_members",
    "moderate_members",
    "view_audit_log",
    "manage_messages",
    "mention_everyone",
    "view_channel",
    "send_messages",
    "attach_files",
    "add_reactions",
    "connect",
    "speak",
    "stream",
    "mute_members",
    "move_members",
];

pub const EVERYONE: &str = "everyone";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Perms(u32);

impl Perms {
    pub const ALL: Perms = Perms((1 << PERMISSIONS.len()) - 1);

    fn of<S: AsRef<str>>(names: &[S]) -> Perms {
        Perms(
            names
                .iter()
                .filter_map(|n| PERMISSIONS.iter().position(|p| *p == n.as_ref()))
                .fold(0, |bits, i| bits | 1 << i),
        )
    }

    pub fn has(self, name: &str) -> bool {
        let want = Perms::of(&[name]).0;
        want != 0 && self.0 & want == want
    }

    /// The permissions in `self`, in spec order.
    pub fn names(self) -> Vec<String> {
        PERMISSIONS
            .iter()
            .enumerate()
            .filter(|(i, _)| self.0 & 1 << i != 0)
            .map(|(_, p)| p.to_string())
            .collect()
    }
}

/// A `["role", id, name, color, attr…]` tag; attributes are permissions, `hoist`, `mentionable`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Role {
    pub id: String,
    pub name: String,
    pub color: String,
    pub attrs: Vec<String>,
}

impl Role {
    pub fn perms(&self) -> Perms {
        Perms::of(&self.attrs)
    }
}

/// An `["allow" | "deny", target, perm…]` tag of a channel: target is a role id, `everyone` or a key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Override {
    pub allow: bool,
    pub target: String,
    pub perms: Perms,
}

/// Who asks: a key (hex), its role ids, its timeout (0: none).
#[derive(Debug, Clone, Default)]
pub struct Member<'a> {
    pub pubkey: &'a str,
    pub roles: &'a [String],
    pub timeout: u64,
}

/// A community's owner (hex) and roles, highest first, `everyone` last.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Policy {
    pub owner: String,
    pub roles: Vec<Role>,
}

/// The roles of a new community, and of one from before ChatON roles (spec § 9).
pub fn default_roles() -> Vec<Role> {
    let role = |id: &str, name: &str, attrs: &[&str]| Role {
        id: id.into(),
        name: name.into(),
        color: String::new(),
        attrs: attrs.iter().map(|a| a.to_string()).collect(),
    };
    vec![
        role("admin", "Admin", &["hoist", "administrator"]),
        role(
            "moderator",
            "Modérateur",
            &[
                "hoist",
                "manage_messages",
                "kick_members",
                "ban_members",
                "moderate_members",
                "create_invite",
                "view_audit_log",
                "mute_members",
                "move_members",
            ],
        ),
        role(
            EVERYONE,
            "@everyone",
            &[
                "view_channel",
                "send_messages",
                "attach_files",
                "add_reactions",
                "connect",
                "speak",
                "stream",
            ],
        ),
    ]
}

/// The roles of a 39003; `None` unless in ChatON's format (NIP-29 `["role", name, desc]` before it).
pub fn parse_roles<T: AsRef<[String]>>(tags: impl IntoIterator<Item = T>) -> Option<Vec<Role>> {
    let mut roles = Vec::new();
    for tag in tags {
        match tag.as_ref() {
            [k, id, name, color, attrs @ ..] if k == "role" => roles.push(Role {
                id: id.clone(),
                name: name.clone(),
                color: color.clone(),
                attrs: attrs.to_vec(),
            }),
            [k, ..] if k == "role" => return None,
            _ => {}
        }
    }
    (roles.last()?.id == EVERYONE).then_some(roles)
}

/// The allow and deny tags of a channel.
pub fn parse_overrides<T: AsRef<[String]>>(tags: impl IntoIterator<Item = T>) -> Vec<Override> {
    tags.into_iter()
        .filter_map(|tag| match tag.as_ref() {
            [k, target, perms @ ..] if k == "allow" || k == "deny" => Some(Override {
                allow: k == "allow",
                target: target.clone(),
                perms: Perms::of(perms),
            }),
            _ => None,
        })
        .collect()
}

impl Policy {
    fn role(&self, id: &str) -> Option<&Role> {
        self.roles.iter().find(|r| r.id == id)
    }

    /// `m`'s permissions in the community (`channel` None) or in a channel with
    /// these overrides, at `now` (unix seconds).
    pub fn effective(&self, m: &Member, channel: Option<&[Override]>, now: u64) -> Perms {
        if m.pubkey == self.owner {
            return Perms::ALL;
        }
        let mut base = self.role(EVERYONE).map(Role::perms).unwrap_or_default().0;
        for id in m.roles.iter().filter(|id| *id != EVERYONE) {
            if let Some(r) = self.role(id) {
                base |= r.perms().0;
            }
        }
        if Perms(base).has("administrator") {
            return Perms::ALL;
        }
        if let Some(overrides) = channel {
            let mut apply = |matches: &dyn Fn(&str) -> bool| {
                let (mut allow, mut deny) = (0, 0);
                for o in overrides.iter().filter(|o| matches(&o.target)) {
                    match o.allow {
                        true => allow |= o.perms.0,
                        false => deny |= o.perms.0,
                    }
                }
                base = base & !deny | allow;
            };
            apply(&|t| t == EVERYONE);
            apply(&|t| t != EVERYONE && m.roles.iter().any(|r| r == t));
            apply(&|t| t == m.pubkey);
            if !Perms(base).has("view_channel") {
                return Perms::default();
            }
            if !Perms(base).has("connect") {
                base &= !(Perms::of(&["speak", "stream"]).0);
            }
        }
        if m.timeout > now {
            return Perms(base & Perms::of(&["view_channel"]).0);
        }
        Perms(base)
    }

    /// Position of `m`'s highest role (lower is higher); the owner is above all.
    fn rank(&self, m: &Member) -> isize {
        if m.pubkey == self.owner {
            return -1;
        }
        let mut best = self.roles.len() as isize;
        for (i, r) in self.roles.iter().enumerate() {
            if r.id == EVERYONE {
                best = best.min(i as isize);
            } else if m.roles.contains(&r.id) {
                return i as isize;
            }
        }
        best
    }

    /// Ids of the roles strictly below `m`'s highest, `everyone` included.
    pub fn roles_below(&self, m: &Member) -> Vec<String> {
        let rank = self.rank(m);
        self.roles
            .iter()
            .enumerate()
            .filter(|(i, _)| *i as isize > rank)
            .map(|(_, r)| r.id.clone())
            .collect()
    }

    /// `a` is strictly above `b`.
    pub fn outranks(&self, a: &Member, b: &Member) -> bool {
        self.rank(a) < self.rank(b)
    }
}
