use weekend_db::{
    Error, Result,
    crdt::{Actor, AddWinsSet, GrowCounter, GrowSet, MaxRegister, MinRegister, Record, Resolution},
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Record, Resolution)]
#[record(
    key = b"course-progress",
    format = if AUTO_RESOLVE { b"COURSES\x06" } else { b"COURSES\x05" },
    fields(completed, practice_minutes, planned, weekly_goal, session_limit),
    resolve = AUTO_RESOLVE,
)]
pub struct StudyProgress<const AUTO_RESOLVE: bool = true> {
    pub planned: AddWinsSet,
    pub completed: GrowSet,
    pub practice_minutes: GrowCounter,
    #[resolve]
    pub weekly_goal: MaxRegister,
    #[resolve]
    pub session_limit: MinRegister,
}

impl<const AUTO_RESOLVE: bool> StudyProgress<AUTO_RESOLVE> {
    pub fn effective_weekly_goal(&self) -> Option<u64> {
        self.weekly_goal.effective_value()
    }

    pub fn resolve_goal(
        &mut self,
        actor: Actor,
        reviewed: MaxRegister,
        minutes: u64,
    ) -> Result<()> {
        if self.weekly_goal != reviewed {
            return Err(Error::Conflict);
        }
        self.weekly_goal.assign(actor, minutes)
    }
}
