use crate::client::Lettermint;
use crate::error::Result;
use crate::generated::operations as ops;
use crate::generated::types::{
    CreateTeamInviteData, CreateTeamRoleData, DeleteSuppressionResponse, DeleteTeamRoleQuery,
    DuplicateTeamRoleData, GetStatsQuery, GetTeamQuery, ListSuppressionsQuery,
    ListSuppressionsResponse, ListTeamInvitesQuery, ListTeamInvitesResponse, ListTeamMembersQuery,
    ListTeamMembersResponse, MessageResponse, StatsData, StoreSuppressionData,
    SuppressedRecipientData, SuppressionStoreResponse, TeamData, TeamInviteData, TeamMemberData,
    TeamMutationResponse, TeamRoleData, TeamRoleListResponse, TeamUsageDetailData, UpdateTeamData,
    UpdateTeamInviteData, UpdateTeamMemberAssignmentData, UpdateTeamRoleData,
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

/// Pending invitations to the team. Team token. Returned by [`Team::invites`].
#[derive(Clone)]
pub struct TeamInvites {
    client: Lettermint,
}

super::opaque_debug!(Stats, Suppressions, Team, TeamMembers, TeamInvites);

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

    /// Pending invitations to the team.
    pub fn invites(&self) -> TeamInvites {
        TeamInvites {
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

    /// A team role.
    pub async fn retrieve_role(&self, role_id: &str) -> Result<TeamRoleData> {
        self.client
            .call::<ops::GetTeamRole>(
                "team.retrieve_role",
                &[role_id],
                &(),
                None,
                CallOptions::default(),
            )
            .await
    }

    /// Creates a custom role. The plan must include custom roles, the permissions must include
    /// the permissions they depend on, and role names are unique within the team.
    pub async fn create_role(&self, body: &CreateTeamRoleData) -> Result<TeamRoleData> {
        self.client
            .call::<ops::CreateTeamRole>(
                "team.create_role",
                &[],
                &(),
                Some(body),
                CallOptions::default(),
            )
            .await
    }

    /// Replaces the name, description and complete permission set of a custom role. The change
    /// applies to every member, invitation and SCIM group that holds the role. Built-in roles
    /// cannot be changed.
    pub async fn update_role(
        &self,
        role_id: &str,
        body: &UpdateTeamRoleData,
    ) -> Result<TeamRoleData> {
        self.client
            .call::<ops::UpdateTeamRole>(
                "team.update_role",
                &[role_id],
                &(),
                Some(body),
                CallOptions::default(),
            )
            .await
    }

    /// Deletes a custom role. While the role is still assigned to members, invitations or SCIM
    /// groups, `query.replacement_role_id` names the role that receives those assignments.
    /// Built-in roles cannot be deleted.
    pub async fn delete_role(
        &self,
        role_id: &str,
        query: &DeleteTeamRoleQuery,
    ) -> Result<MessageResponse> {
        self.client
            .call::<ops::DeleteTeamRole>(
                "team.delete_role",
                &[role_id],
                query,
                None,
                CallOptions::default(),
            )
            .await
    }

    /// Copies a role under a new name. Without a description, the copy keeps the description of
    /// the source role. The Owner role cannot be duplicated.
    pub async fn duplicate_role(
        &self,
        role_id: &str,
        body: &DuplicateTeamRoleData,
    ) -> Result<TeamRoleData> {
        self.client
            .call::<ops::DuplicateTeamRole>(
                "team.duplicate_role",
                &[role_id],
                &(),
                Some(body),
                CallOptions::default(),
            )
            .await
    }
}

impl TeamMembers {
    /// Lists team members, one page at a time. `query.filter_project_id` returns only the
    /// members who can access that project: members with access to all projects and members
    /// selected for it.
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

    /// Removes a member from the team. The Owner cannot be removed.
    pub async fn remove(&self, user_id: &str) -> Result<MessageResponse> {
        self.client
            .call::<ops::DeleteTeamMember>(
                "team.members.remove",
                &[user_id],
                &(),
                None,
                CallOptions::default(),
            )
            .await
    }
}

impl TeamInvites {
    /// Lists pending invitations, one page at a time.
    pub async fn list(&self, query: &ListTeamInvitesQuery) -> Result<ListTeamInvitesResponse> {
        self.client
            .call::<ops::ListTeamInvites>(
                "team.invites.list",
                &[],
                query,
                None,
                CallOptions::default(),
            )
            .await
    }

    /// Iterates over every pending invitation, following `next_cursor`.
    pub fn iterate(&self, query: &ListTeamInvitesQuery) -> Paginator<TeamInviteData> {
        self.client
            .paginate::<ops::ListTeamInvites>("team.invites.iterate", &[], query)
    }

    /// A pending invitation.
    pub async fn retrieve(&self, invite_id: &str) -> Result<TeamInviteData> {
        self.client
            .call::<ops::GetTeamInvite>(
                "team.invites.retrieve",
                &[invite_id],
                &(),
                None,
                CallOptions::default(),
            )
            .await
    }

    /// Invites a person to the team with a role and project access, and emails the invitation.
    /// The team needs a free seat, and the Owner role cannot be assigned.
    pub async fn create(&self, body: &CreateTeamInviteData) -> Result<TeamInviteData> {
        self.client
            .call::<ops::CreateTeamInvite>(
                "team.invites.create",
                &[],
                &(),
                Some(body),
                CallOptions::default(),
            )
            .await
    }

    /// Replaces the email address, role and project access of a pending invitation. A new
    /// invitation email is sent only when the address changes.
    pub async fn update(
        &self,
        invite_id: &str,
        body: &UpdateTeamInviteData,
    ) -> Result<TeamInviteData> {
        self.client
            .call::<ops::UpdateTeamInvite>(
                "team.invites.update",
                &[invite_id],
                &(),
                Some(body),
                CallOptions::default(),
            )
            .await
    }

    /// Revokes a pending invitation.
    pub async fn revoke(&self, invite_id: &str) -> Result<MessageResponse> {
        self.client
            .call::<ops::DeleteTeamInvite>(
                "team.invites.revoke",
                &[invite_id],
                &(),
                None,
                CallOptions::default(),
            )
            .await
    }
}
