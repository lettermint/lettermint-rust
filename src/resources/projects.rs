use crate::client::Lettermint;
use crate::error::Result;
use crate::generated::operations as ops;
use crate::generated::types::{
    GetProjectQuery, GetReportForwardingResponse, ListProjectsQuery, ListProjectsResponse,
    MessageResponse, ProjectCreatedData, ProjectData, ProjectListData, ProjectMutationResponse,
    ReportForwardingRequest, ResendReportForwardingCodeResponse, RotateProjectTokenResponse,
    StoreProjectData, UpdateProjectData, UpdateReportForwardingResponse,
    VerifyReportForwardingRequest, VerifyReportForwardingResponse,
};
use crate::pagination::Paginator;
use crate::transport::CallOptions;

/// Projects. Team token. Returned by [`Lettermint::projects`].
#[derive(Clone)]
pub struct Projects {
    client: Lettermint,
}

/// DMARC and complaint report forwarding of a project. Team token. Returned by
/// [`Projects::report_forwarding`].
#[derive(Clone)]
pub struct ReportForwarding {
    client: Lettermint,
}

super::opaque_debug!(Projects, ReportForwarding);

impl Projects {
    pub(crate) fn new(client: Lettermint) -> Self {
        Self { client }
    }

    /// Report forwarding of a project.
    pub fn report_forwarding(&self) -> ReportForwarding {
        ReportForwarding {
            client: self.client.clone(),
        }
    }

    /// Lists projects, one page at a time.
    pub async fn list(&self, query: &ListProjectsQuery) -> Result<ListProjectsResponse> {
        self.client
            .call::<ops::ListProjects>("projects.list", &[], query, None, CallOptions::default())
            .await
    }

    /// Iterates over every project, following `next_cursor`.
    pub fn iterate(&self, query: &ListProjectsQuery) -> Paginator<ProjectListData> {
        self.client
            .paginate::<ops::ListProjects>("projects.iterate", &[], query)
    }

    /// Creates a project. The response holds its sending token once (`api_token`).
    pub async fn create(&self, body: &StoreProjectData) -> Result<ProjectCreatedData> {
        self.client
            .call::<ops::CreateProject>(
                "projects.create",
                &[],
                &(),
                Some(body),
                CallOptions::default(),
            )
            .await
    }

    /// A project.
    pub async fn retrieve(&self, project_id: &str, query: &GetProjectQuery) -> Result<ProjectData> {
        self.client
            .call::<ops::GetProject>(
                "projects.retrieve",
                &[project_id],
                query,
                None,
                CallOptions::default(),
            )
            .await
    }

    /// Updates a project.
    pub async fn update(
        &self,
        project_id: &str,
        body: &UpdateProjectData,
    ) -> Result<ProjectMutationResponse> {
        self.client
            .call::<ops::UpdateProject>(
                "projects.update",
                &[project_id],
                &(),
                Some(body),
                CallOptions::default(),
            )
            .await
    }

    /// Deletes a project.
    pub async fn delete(&self, project_id: &str) -> Result<MessageResponse> {
        self.client
            .call::<ops::DeleteProject>(
                "projects.delete",
                &[project_id],
                &(),
                None,
                CallOptions::default(),
            )
            .await
    }

    /// Rotates the project's legacy sending token.
    #[deprecated(note = "the API marks this endpoint as legacy")]
    pub async fn rotate_token(&self, project_id: &str) -> Result<RotateProjectTokenResponse> {
        self.client
            .call::<ops::RotateProjectToken>(
                "projects.rotate_token",
                &[project_id],
                &(),
                None,
                CallOptions::default(),
            )
            .await
    }
}

impl ReportForwarding {
    /// The report forwarding settings of a project.
    pub async fn retrieve(&self, project_id: &str) -> Result<GetReportForwardingResponse> {
        self.client
            .call::<ops::GetReportForwarding>(
                "projects.report_forwarding.retrieve",
                &[project_id],
                &(),
                None,
                CallOptions::default(),
            )
            .await
    }

    /// Sets the forwarding destination.
    pub async fn update(
        &self,
        project_id: &str,
        body: &ReportForwardingRequest,
    ) -> Result<UpdateReportForwardingResponse> {
        self.client
            .call::<ops::UpdateReportForwarding>(
                "projects.report_forwarding.update",
                &[project_id],
                &(),
                Some(body),
                CallOptions::default(),
            )
            .await
    }

    /// Disables report forwarding (HTTP 204).
    pub async fn delete(&self, project_id: &str) -> Result<()> {
        self.client
            .call::<ops::DeleteReportForwarding>(
                "projects.report_forwarding.delete",
                &[project_id],
                &(),
                None,
                CallOptions::default(),
            )
            .await
    }

    /// Confirms the destination with the emailed code.
    pub async fn verify(
        &self,
        project_id: &str,
        body: &VerifyReportForwardingRequest,
    ) -> Result<VerifyReportForwardingResponse> {
        self.client
            .call::<ops::VerifyReportForwarding>(
                "projects.report_forwarding.verify",
                &[project_id],
                &(),
                Some(body),
                CallOptions::default(),
            )
            .await
    }

    /// Sends the verification code again.
    pub async fn resend_code(
        &self,
        project_id: &str,
    ) -> Result<ResendReportForwardingCodeResponse> {
        self.client
            .call::<ops::ResendReportForwardingCode>(
                "projects.report_forwarding.resend_code",
                &[project_id],
                &(),
                None,
                CallOptions::default(),
            )
            .await
    }
}
