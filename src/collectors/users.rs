use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use sysinfo::{Gid, Groups, Uid, Users};

const USER_LIST_RETRY: Duration = Duration::from_secs(5);

/// Resolves user ids to names, caching hits. The `Users` list is reloaded after a
/// miss, but at most every few seconds, because a burst of new processes owned by
/// an unlisted account (LDAP users, Windows service SIDs) would otherwise reload it
/// once per process.
pub struct UserNames {
    users: Users,
    groups: Groups,
    names: HashMap<Uid, Arc<str>>,
    last_list_refresh: Instant,
    unknown: Arc<str>,
}

impl UserNames {
    pub fn new() -> Self {
        Self {
            users: Users::new_with_refreshed_list(),
            groups: Groups::new_with_refreshed_list(),
            names: HashMap::new(),
            last_list_refresh: Instant::now(),
            unknown: Arc::from("-"),
        }
    }

    pub fn resolve(&mut self, uid: Option<&Uid>, now: Instant) -> Arc<str> {
        let Some(uid) = uid else {
            return self.unknown.clone();
        };
        if let Some(name) = self.names.get(uid) {
            return name.clone();
        }
        if self.users.get_user_by_id(uid).is_none()
            && now.duration_since(self.last_list_refresh) >= USER_LIST_RETRY
        {
            self.users.refresh();
            self.last_list_refresh = now;
        }
        match self.users.get_user_by_id(uid) {
            Some(user) => {
                let name: Arc<str> = Arc::from(user.name());
                self.names.insert(uid.clone(), name.clone());
                name
            }
            None => Arc::from(uid.to_string()),
        }
    }

    pub fn group(&self, gid: &Gid) -> Option<&str> {
        self.groups
            .list()
            .iter()
            .find(|g| g.id() == gid)
            .map(|g| g.name())
    }
}
