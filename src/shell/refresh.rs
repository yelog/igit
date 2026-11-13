use super::*;

#[derive(Default)]
pub(super) struct State {
    pub(super) background: bool,
    dirty: [bool; Tab::DISPLAY_ORDER.len()],
}

impl State {
    pub(super) fn request(&mut self, tab: Tab) {
        self.dirty[tab as usize] = true;
    }

    pub(super) fn pending(&self, tab: Tab) -> bool {
        self.dirty[tab as usize]
    }

    pub(super) fn clear(&mut self, tab: Tab) {
        self.dirty[tab as usize] = false;
    }
}

impl Shell {
    pub(super) fn enable_background_loading(&mut self) {
        self.refresh_state.background = true;
        self.merge.enable_background_loading();
        self.stash.enable_background_loading();
        self.remote.enable_background_loading();
        if self.log_load.ready() {
            self.log.enable_background_loading();
        }
    }

    fn refresh_allowed(&self, tab: Tab) -> bool {
        if self.pop.is_some()
            || self.git_busy()
            || self.pending_keys.is_some()
            || self.notification_prefix.is_some()
            || self.window_prefix.is_some()
            || self.path_copy_pending
        {
            return false;
        }
        match tab {
            Log => self.log.refresh_allowed(),
            Branch => {
                !self.branch.editing()
                    && self.branch.selected_text().is_none()
                    && self.branch.key_sequence().is_none()
                    && !self.branch.panes.dragging()
            }
            Push => self.push.refresh_allowed(),
            Stash => self.stash.refresh_allowed(),
            Remote => self.remote.refresh_allowed(),
            Conflicts => self.merge.refresh_allowed(),
            Commit => false,
        }
    }

    pub(super) fn poll_refresh_requests(&mut self) -> bool {
        let mut changed = false;
        for tab in [Log, Branch, Push, Stash, Remote, Conflicts] {
            if !self.refresh_state.pending(tab)
                || tab != self.tab && tab != Conflicts
                || !self.refresh_allowed(tab)
                || tab == Log && self.log_load.pending()
            {
                continue;
            }
            self.refresh_state.clear(tab);
            let result = match tab {
                Log => {
                    self.reload_log();
                    Ok(())
                }
                Branch => self.branch.refresh(&self.ctx),
                Push => self.push.refresh(&self.ctx),
                Stash => self.stash.refresh(&self.ctx),
                Remote => self.remote.refresh(&self.ctx),
                Conflicts => self.merge.refresh(&self.ctx),
                Commit => unreachable!(),
            };
            if let Err(error) = result {
                self.notify(NoticeLevel::Error, tab.name(), format!("{error:#}"));
            }
            changed = true;
        }
        changed
    }

    pub(super) fn poll_list_refreshes(&mut self) -> bool {
        let mut changed = false;
        for tab in [Stash, Remote, Conflicts] {
            if !self.refresh_allowed(tab) {
                continue;
            }
            let result = match tab {
                Stash => self.stash.poll_loading(&self.ctx),
                Remote => self.remote.poll_loading(&self.ctx),
                Conflicts => self.merge.poll_loading(&self.ctx),
                _ => unreachable!(),
            };
            match result {
                Ok(redraw) => {
                    changed |= redraw && (self.tab == tab || tab == Conflicts);
                }
                Err(error) => {
                    self.notify(NoticeLevel::Error, tab.name(), format!("{error:#}"));
                    changed = true;
                }
            }
        }
        if self.tab == Conflicts
            && !self.merge.reads_pending()
            && !self.refresh_state.pending(Conflicts)
            && !self.tab_available(Conflicts)
        {
            self.go(self.main_tab);
            changed = true;
        }
        changed
    }
}
