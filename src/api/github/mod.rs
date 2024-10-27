use graphql_client::{reqwest::post_graphql, GraphQLQuery, Response};
use reqwest::IntoUrl;
use thiserror::Error;

pub const GRAPHQL_ENDPOINT: &str = "https://api.github.com/graphql";

pub struct GitHubClient<U> {
    client: reqwest::Client,
    endpoint: U,
}

#[derive(Error, Debug)]
pub enum ApiResponseError {
    #[error("The response has no data")]
    NoData,
    #[error("Missing expected node {0}")]
    EmptyNode(String),
}

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "src/api/github/schema.docs.graphql",
    query_path = "src/api/github/login.graphql",
    response_derives = "Debug"
)]
struct LoginQuery;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "src/api/github/schema.docs.graphql",
    query_path = "src/api/github/owned-repos.graphql",
    response_derives = "Clone"
)]
struct StarredOwnReposQuery;

pub type ResponseRepoEntry = starred_own_repos_query::StarredOwnReposQueryViewerRepositoriesNodes;

// Required in CommitHistoryQuery
type DateTime = String;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "src/api/github/schema.docs.graphql",
    query_path = "src/api/github/commit-history.graphql",
    response_derives = "Clone,Debug"
)]
struct CommitHistoryQuery;

pub type CommitEntry =
    commit_history_query::CommitHistoryQueryRepositoryDefaultBranchRefTargetOnCommitHistoryNodes;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "src/api/github/schema.docs.graphql",
    query_path = "src/api/github/stargazers.graphql",
    response_derives = "Clone,Debug"
)]
struct StargazersQuery;

pub type StargazerEntry = stargazers_query::StargazersQueryRepositoryStargazersEdges;

impl<U: IntoUrl + Copy> GitHubClient<U> {
    // Most of this code has been just stolen from
    // https://github.com/graphql-rust/graphql-client/blob/main/examples/github/examples/github.rs
    pub fn new(endpoint: U) -> anyhow::Result<Self> {
        let github_api_token =
            std::env::var("GITHUB_API_TOKEN").expect("Missing GITHUB_API_TOKEN env var");

        let client = reqwest::Client::builder()
            .user_agent("github-statistics")
            .default_headers(
                std::iter::once((
                    reqwest::header::AUTHORIZATION,
                    reqwest::header::HeaderValue::from_str(&format!("Bearer {}", github_api_token))
                        .unwrap(),
                ))
                .collect(),
            )
            .build()?;

        Ok(Self { client, endpoint })
    }

    pub async fn get_owned_repositories(
        &self,
        after: Option<String>,
    ) -> anyhow::Result<Response<starred_own_repos_query::ResponseData>> {
        let variables = starred_own_repos_query::Variables { after };

        let response =
            post_graphql::<StarredOwnReposQuery, U>(&self.client, self.endpoint, variables).await?;

        Ok(response)
    }

    pub async fn get_all_starred_own_repositories(
        &self,
    ) -> anyhow::Result<(String, Vec<ResponseRepoEntry>)> {
        let mut result = Vec::new();
        let mut cursor: Option<String> = None;

        loop {
            let response = self.get_owned_repositories(cursor).await?;
            let data = response.data.unwrap().viewer;
            match data.repositories.nodes {
                None => {
                    break Ok((data.login, result));
                }
                Some(mut vec) => {
                    let mut items = vec
                        .iter_mut()
                        .filter(|i| i.is_some())
                        .map(|i| i.as_mut().unwrap().clone())
                        .take_while(|item| item.stargazer_count > 0)
                        .collect::<Vec<_>>();
                    let end = vec.len() != items.len();
                    result.append(&mut items);
                    if !data.repositories.page_info.has_next_page || end {
                        break Ok((data.login, result));
                    }
                    cursor = data.repositories.page_info.end_cursor;
                }
            }
        }
    }

    pub async fn get_stargazers(
        &self,
        owner: String,
        name: String,
        count: i64,
        before: Option<String>,
    ) -> anyhow::Result<stargazers_query::StargazersQueryRepositoryStargazers> {
        let variables = stargazers_query::Variables {
            owner,
            name,
            count,
            before,
        };

        let response =
            post_graphql::<StargazersQuery, U>(&self.client, self.endpoint, variables).await?;

        let stargazers = response
            .data
            .ok_or(ApiResponseError::NoData)?
            .repository
            .ok_or(ApiResponseError::EmptyNode(format!("repository")))?
            .stargazers;

        Ok(stargazers)
    }

    pub async fn get_stargazers_after_count(
        &self,
        owner: String,
        name: String,
        after_count: i64,
        expected_total_count: i64,
        page_size: i64,
    ) -> anyhow::Result<(i64, Vec<StargazerEntry>)> {
        let mut result = Vec::new();
        let mut cursor = None;
        let mut accum_count = after_count;
        let mut total_count = expected_total_count;

        loop {
            let count = std::cmp::min(total_count - accum_count + 5, page_size);
            let page = self
                .get_stargazers(owner.to_owned(), name.to_owned(), count, cursor.to_owned())
                .await?;

            total_count = page.total_count;

            let edges = page
                .edges
                .ok_or(ApiResponseError::EmptyNode(format!("edges")))?;

            for edge in edges.iter() {
                match edge {
                    Some(item) => {
                        result.push(item.clone());
                        accum_count += 1;
                        if accum_count == total_count {
                            break;
                        }
                    }
                    None => {}
                }
            }

            if !page.page_info.has_previous_page || accum_count >= total_count {
                break;
            }

            cursor = page.page_info.start_cursor;
        }

        Ok((total_count, result))
    }

    pub async fn get_commit_history(
        &self,
        owner: String,
        name: String,
        after: Option<String>,
    ) -> anyhow::Result<
        commit_history_query::CommitHistoryQueryRepositoryDefaultBranchRefTargetOnCommitHistory,
    > {
        let variables = commit_history_query::Variables { owner, name, after };

        let response =
            post_graphql::<CommitHistoryQuery, U>(&self.client, self.endpoint, variables).await?;

        let target = response
            .data
            .ok_or(ApiResponseError::NoData)?
            .repository
            .ok_or(ApiResponseError::EmptyNode(format!("repository")))?
            .default_branch_ref
            .ok_or(ApiResponseError::EmptyNode(format!("default_branch_ref")))?
            .target
            .ok_or(ApiResponseError::EmptyNode(format!("target")))?;

        match target {
            commit_history_query::CommitHistoryQueryRepositoryDefaultBranchRefTarget::Commit(
                head,
            ) => Ok(head.history),
            _ => Err(anyhow::format_err!("non-commit head")),
        }
    }

    pub async fn get_first_commits(
        &self,
        owner: String,
        name: String,
        count_limit: usize,
    ) -> anyhow::Result<Option<Vec<CommitEntry>>> {
        let mut cursor = None;
        let mut previous_cursor = None;

        let mut count = 0;
        let mut commits = Vec::new();

        loop {
            let history = self
                .get_commit_history(owner.clone(), name.clone(), cursor)
                .await?;

            let page_info = history.page_info;
            if page_info.has_next_page {
                previous_cursor = page_info.start_cursor;
                cursor = page_info.end_cursor;
            } else {
                let nodes = history
                    .nodes
                    .ok_or(ApiResponseError::EmptyNode(format!("nodes")))?;
                for node in nodes.iter() {
                    if let Some(commit_node) = node {
                        commits.push(commit_node.clone());
                        count += 1;
                        if count == count_limit {
                            // stop scanning nodes
                            break;
                        };
                    };
                }

                if count < count_limit && previous_cursor.is_some() {
                    let previous_history = self
                        .get_commit_history(owner.clone(), name.clone(), previous_cursor)
                        .await?;
                    let previous_nodes =
                        previous_history
                            .nodes
                            .ok_or(ApiResponseError::EmptyNode(format!(
                                "nodes in previous history"
                            )))?;
                    for node in previous_nodes.iter() {
                        if let Some(commit_node) = node {
                            commits.push(commit_node.clone());
                            count += 1;
                            if count == count_limit {
                                // stop scanning nodes
                                break;
                            };
                        };
                    }
                }

                // no more cursor
                break;
            }
        }

        if count > 0 {
            Ok(Some(commits))
        } else {
            Ok(None)
        }
    }
}

impl GitHubClient<&str> {
    pub fn default() -> anyhow::Result<Self> {
        Self::new(GRAPHQL_ENDPOINT)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_client<'a>() -> GitHubClient<&'a str> {
        dotenvy::dotenv().expect("dotenv");
        GitHubClient::default().expect("github client")
    }

    #[tokio::test]
    #[ignore]
    async fn test_starred_own_repositories() {
        let client = setup_client();
        let result = client.get_all_starred_own_repositories().await;
        assert!(result.is_ok());
        let (_, repositories) = result.unwrap();
        assert!(repositories.len() > 0);
    }

    #[tokio::test]
    #[ignore]
    async fn test_stargazers() {
        let client = setup_client();
        let after_count = 80;
        let expected_total_count = 105;
        let page_size = 20;
        let result = client
            .get_stargazers_after_count(
                format!("akirak"),
                format!("flake-templates"),
                after_count,
                expected_total_count,
                page_size,
            )
            .await;
        assert!(result.is_ok());
        let (new_total_count, items) = result.unwrap();
        assert_eq!(new_total_count, after_count + items.len() as i64);
    }

    #[tokio::test]
    #[ignore]
    async fn test_first_commits() {
        let client = setup_client();

        let result = client
            .get_first_commits(format!("akirak"), format!("twind.el"), 3)
            .await;

        assert!(result.is_ok());
        assert!(result.as_ref().unwrap().is_some());

        let commits = result.unwrap().unwrap();
        assert_eq!(commits.len(), 3);
    }
}
