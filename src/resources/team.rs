use crate::client::Lettermint;
use crate::error::Result;
use crate::generated::operations as ops;
use crate::generated::types::{
    DeleteSuppressionResponse, GetStatsQuery, GetTeamQuery, ListSuppressionsQuery,
    ListSuppressionsResponse, ListTeamMembersQuery, ListTeamMembersResponse, StatsData,
    StoreSuppressionData, SuppressedRecipientData, SuppressionStoreResponse, TeamData,
    TeamMemberData, TeamMutationResponse, TeamRoleListResponse, TeamUsageDetailData,
    UpdateTeamData, UpdateTeamMemberAssignmentData,
};
use crate::pagination::Paginator;
use crate::transport::CallOptions;

/// Sending statistics. Team token. Returned by [`Lettermint::stats`].
#[derive(Clone)]
pub struct Stats {
    client: Lettermint,
}

/// The suppression list. Team token. Returned by [`Lettermint::suppressions`].
#[derive(Clone)]
pub struct Suppressions {
    client: Lettermint,
}

/// The team of the token. Team token. Returned by [`Lettermint::team`].
#[derive(Clone)]
pub struct Team {
    client: Lettermint,
}

/// Team members. Team token. Returned by [`Team::members`].
#[derive(Clone)]
pub struct TeamMembers {
    client: Lettermint,
}

super::opaque_debug!(Stats, Suppressions, Team, TeamMembers);

impl Stats {
    pub(crate) fn new(client: Lettermint) -> Self {
        Self { client }
    }

    /// Daily statistics between `from` and `to` (`Y-m-d`, at most 90 days).
    pub async fn retrieve(&self, query: &GetStatsQuery) -> Result<StatsData> {
        self.client
            .call::<ops::GetStats>("stats.retrieve", &[], query, None, CallOptions::default())
            .await
    }
}

impl Suppressions {
    pub(crate) fn new(client: Lettermint) -> Self {
        Self { client }
    }

    /// Lists suppressions, one page at a time.
    pub async fn list(&self, query: &ListSuppressionsQuery) -> Result<ListSuppressionsResponse> {
        self.client
            .call::<ops::ListSuppressions>(
                "suppressions.list",
                &[],
                query,
                None,
                CallOptions::default(),
            )
            .await
    }

    /// Iterates over every suppression, following `next_cursor`.
    pub fn iterate(&self, query: &ListSuppressionsQuery) -> Paginator<SuppressedRecipientData> {
        self.client
            .paginate::<ops::ListSuppressions>("suppressions.iterate", &[], query)
    }

    /// Adds addresses or domains to the suppression list.
    pub async fn create(&self, body: &StoreSuppressionData) -> Result<SuppressionStoreResponse> {
        self.client
            .call::<ops::CreateSuppressions>(
                "suppressions.create",
                &[],
                &(),
                Some(body),
                CallOptions::default(),
            )
            .await
    }

    /// Removes a suppression, or opens a review ticket when the removal needs one (HTTP 202).
    pub async fn delete(&self, suppression_id: &str) -> Result<DeleteSuppressionResponse> {
        self.client
            .call::<ops::DeleteSuppression>(
                "suppressions.delete",
                &[suppression_id],
                &(),
                None,
                CallOptions::default(),
            )
            .await
    }
}

impl Team {
    pub(crate) fn new(client: Lettermint) -> Self {
        Self { client }
    }

    /// Team members.
    pub fn members(&self) -> TeamMembers {
        TeamMembers {
            client: self.client.clone(),
        }
    }

    /// The team. `query.include` adds `features` or `addons`.
    pub async fn retrieve(&self, query: &GetTeamQuery) -> Result<TeamData> {
        self.client
            .call::<ops::GetTeam>("team.retrieve", &[], query, None, CallOptions::default())
            .await
    }

    /// Updates the team.
    pub async fn update(&self, body: &UpdateTeamData) -> Result<TeamMutationResponse> {
        self.client
            .call::<ops::UpdateTeam>("team.update", &[], &(), Some(body), CallOptions::default())
            .await
    }

    /// Usage of the current and previous billing periods.
    pub async fn usage(&self) -> Result<TeamUsageDetailData> {
        self.client
            .call::<ops::GetTeamUsage>("team.usage", &[], &(), None, CallOptions::default())
            .await
    }

    /// The roles that can be assigned to members.
    pub async fn roles(&self) -> Result<TeamRoleListResponse> {
        self.client
            .call::<ops::ListTeamRoles>("team.roles", &[], &(), None, CallOptions::default())
            .await
    }
}

impl TeamMembers {
    /// Lists team members, one page at a time.
    pub async fn list(&self, query: &ListTeamMembersQuery) -> Result<ListTeamMembersResponse> {
        self.client
            .call::<ops::ListTeamMembers>(
                "team.members.list",
                &[],
                query,
                None,
                CallOptions::default(),
            )
            .await
    }

    /// Iterates over every team member, following `next_cursor`.
    pub fn iterate(&self, query: &ListTeamMembersQuery) -> Paginator<TeamMemberData> {
        self.client
            .paginate::<ops::ListTeamMembers>("team.members.iterate", &[], query)
    }

    /// A team member.
    pub async fn retrieve(&self, user_id: &str) -> Result<TeamMemberData> {
        self.client
            .call::<ops::GetTeamMember>(
                "team.members.retrieve",
                &[user_id],
                &(),
                None,
                CallOptions::default(),
            )
            .await
    }

    /// Changes a member's role and project access.
    pub async fn update_assignment(
        &self,
        user_id: &str,
        body: &UpdateTeamMemberAssignmentData,
    ) -> Result<TeamMemberData> {
        self.client
            .call::<ops::UpdateTeamMemberAssignment>(
                "team.members.update_assignment",
                &[user_id],
                &(),
                Some(body),
                CallOptions::default(),
            )
            .await
    }
}
