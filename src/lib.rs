//! Business logic of the application.

use db::{get_newly_starred_original_repositories, StargazerEntry};
use duckdb::Connection;
use thiserror::Error;
use tracing::info;

mod api;
pub mod chart;
mod db;

#[derive(Debug, Error)]
enum Error {
    #[error("No meaningful data")]
    NoMeaningfulData,
}

fn check_commit_author(login: &str, commit: &api::github::CommitEntry) -> bool {
    match commit.author {
        None => false,
        Some(ref author) => match author.user {
            None => false,
            Some(ref user) => user.login.eq(login),
        },
    }
}

fn update_star_counts(
    db: &mut Connection,
    repositories: &Vec<api::github::ResponseRepoEntry>,
) -> anyhow::Result<()> {
    db::insert_star_counts(
        db,
        &repositories
            .iter()
            .map(|repo| db::StarCountEntry {
                owner: repo.owner.login.as_str(),
                name: repo.name.as_str(),
                stargazer_count: repo.stargazer_count,
            })
            .collect(),
    )?;
    Ok(())
}

fn update_primary_languages(
    db: &mut Connection,
    repositories: &Vec<api::github::ResponseRepoEntry>,
) -> anyhow::Result<()> {
    for repo in repositories.iter() {
        let primary_language = repo.primary_language.clone().map(|lang| lang.name);
        db::insert_repository_primary_language(
            db,
            repo.owner.login.as_str(),
            repo.name.as_str(),
            &primary_language,
        )?;
    }
    Ok(())
}
async fn update_original_statuses(
    db: &mut Connection,
    github: &api::github::GitHubClient<&str>,
    login: &str,
    repositories: &Vec<api::github::ResponseRepoEntry>,
) -> anyhow::Result<()> {
    let originality_known = db::original_status_keys(db)?;

    for repo in repositories.iter() {
        let owner = repo.owner.login.as_str();
        let name = repo.name.as_str();
        if !originality_known.contains(&(owner.to_string(), name.to_string())) {
            let is_original = owner.eq(login) || {
                let first_commits = github
                    .get_first_commits(owner.to_string(), name.to_string(), 5)
                    .await?;

                match first_commits {
                    None => false,
                    Some(commits) => {
                        let same_login_count = commits
                            .iter()
                            .filter(|commit| check_commit_author(login, commit))
                            .count();
                        (same_login_count / commits.len()) > (1 / 2)
                    }
                }
            };

            if is_original {
                info!(owner, name, "original");
            } else {
                info!(owner, name, "not original");
            }

            db::insert_original_status(&db, owner, name, is_original)?;
        }
    }

    Ok(())
}

const STARGAZERS_PAGE_SIZE: i64 = 20;

async fn update_stargazers(
    db: &mut Connection,
    github: &api::github::GitHubClient<&str>,
) -> anyhow::Result<()> {
    for diff in get_newly_starred_original_repositories(db)?.iter() {
        let owner = diff.owner.as_str();
        let name = diff.name.as_str();

        info!(owner, name, "fetching stargazers");

        let (new_total_count, new_items) = github
            .get_stargazers_after_count(
                owner.to_string(),
                name.to_string(),
                diff.old_count,
                diff.new_count,
                STARGAZERS_PAGE_SIZE,
            )
            .await?;

        db::insert_stargazers(
            db,
            owner,
            name,
            new_items
                .iter()
                .map(|x| StargazerEntry {
                    login: x.node.login.to_owned(),
                    starred_at: x.starred_at.to_owned(),
                })
                .collect(),
        )?;

        if new_total_count > diff.new_count {
            db::update_star_count(db, owner, name, new_total_count)?;
            info!(
                old = diff.new_count,
                new = new_total_count,
                "total stargazer count has been updated"
            );
        }
    }

    Ok(())
}

pub async fn update_database(db: &mut Connection) -> anyhow::Result<()> {
    db::setup(db);

    let github = api::github::GitHubClient::default()?;

    let (login, repositories) = github.get_all_starred_own_repositories().await?;

    info!(
        login,
        count = &repositories.len(),
        "fetched starred repositories"
    );

    update_star_counts(db, &repositories)?;
    update_primary_languages(db, &repositories)?;
    update_original_statuses(db, &github, login.as_str(), &repositories).await?;

    update_stargazers(db, &github).await?;

    info!("finished updating the database");

    Ok(())
}

pub fn render_star_history_by_language(db: &mut Connection, path: &str) -> anyhow::Result<()> {
    let vec = db::collect_star_history_by_language(db, 10)?;

    if vec.len() < 2 {
        Err(Error::NoMeaningfulData)?;
    }

    chart::draw_star_history_by_language(vec, path)?;

    info!(path, "saved the image");

    Ok(())
}

pub fn render_total_star_history(db: &mut Connection, path: &str) -> anyhow::Result<()> {
    let vec = db::collect_total_star_history(db)?;

    if vec.len() < 2 {
        Err(Error::NoMeaningfulData)?;
    }

    chart::draw_total_star_history(vec, path)?;

    info!(path, "saved the image");

    Ok(())
}
