use chrono::NaiveDate;
use duckdb::{params, Connection, DropBehavior};
use std::collections::HashSet;

pub fn setup(conn: &Connection) {
    // The schema is pretty dumb because it is meant for analytic purposes.
    let _ = conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS star_counts (
           owner text NOT NULL,
           name text NOT NULL,
           stargazers int NOT NULL
         );

         TRUNCATE star_counts;

         CREATE TABLE IF NOT EXISTS repository_primary_languages (
           owner text NOT NULL,
           name text NOT NULL,
           primary_language text
         );

         TRUNCATE repository_primary_languages;

         -- Persisted to save API usage.
         CREATE TABLE IF NOT EXISTS original_statuses (
           owner text NOT NULL,
           name text NOT NULL,
           original bool NOT NULL
         );

         -- Persisted to save API usage.
         CREATE TABLE IF NOT EXISTS stargazers (
           owner text NOT NULL,
           name text NOT NULL,
           starred_at timestamp NOT NULL,
           starred_by text NOT NULL,
         );

         -- Drop the obsolete view definition.
         DROP VIEW IF EXISTS total_stars_by_language;

         CREATE VIEW IF NOT EXISTS total_stars_by_language_2 AS
         SELECT
           l.primary_language,
           sum(s.stargazers) AS stargazers
         FROM
           repository_primary_languages l
           INNER JOIN star_counts s ON l.owner = s.owner
             AND l.name = s.name
           INNER JOIN original_statuses o ON l.owner = o.owner
             AND l.name = o.name
         WHERE
           o.original
         GROUP BY
           l.primary_language
         ORDER BY
           stargazers DESC;
         ",
    );
}

#[derive(Debug)]
pub struct StarCountEntry<'a> {
    pub owner: &'a str,
    pub name: &'a str,
    pub stargazer_count: i64,
}

pub fn insert_star_counts<'a>(
    conn: &mut Connection,
    repos: &Vec<StarCountEntry<'a>>,
) -> anyhow::Result<()> {
    let mut app = conn.appender("star_counts")?;

    for repo in repos {
        app.append_row(params![repo.owner, repo.name, repo.stargazer_count])?;
    }

    Ok(())
}

pub fn update_star_count<'a>(
    conn: &mut Connection,
    owner: &str,
    name: &str,
    count: i64,
) -> anyhow::Result<()> {
    conn.execute(
        "DELETE FROM star_counts WHERE OWNER = $1 AND name = $2",
        params![owner, name],
    )?;

    let mut app = conn.appender("star_counts")?;

    app.append_row(params![owner, name, count])?;

    Ok(())
}

pub fn original_status_keys(conn: &Connection) -> anyhow::Result<HashSet<(String, String)>> {
    let mut stmt = conn.prepare("SELECT owner, name FROM original_statuses")?;
    let mut rows = stmt.query([])?;

    let mut set = HashSet::new();
    while let Some(row) = rows.next()? {
        set.insert((row.get(0)?, row.get(1)?));
    }

    Ok(set)
}

pub fn insert_original_status(
    conn: &Connection,
    owner: &str,
    name: &str,
    is_original: bool,
) -> anyhow::Result<()> {
    let mut app = conn.appender("original_statuses")?;

    app.append_row(params![owner, name, is_original])?;

    Ok(())
}

#[derive(Debug)]
pub struct StarCountDiff {
    pub owner: String,
    pub name: String,
    pub new_count: i64,
    pub old_count: i64,
}

pub fn get_newly_starred_original_repositories(
    conn: &Connection,
) -> anyhow::Result<Vec<StarCountDiff>> {
    // Here, `new_count > old_count` is used rather than
    // `new_count <> old_count`. This is because there are cases where
    // `new_count < old_count` is true if some users remove stars.
    let mut stmt = conn.prepare(
        "SELECT
              new.owner,
              new.name,
              new.stargazers AS new_count,
              count(old.starred_by) AS old_count
            FROM
              star_counts new
              INNER JOIN original_statuses orig ON new.owner = orig.owner
                AND new.name = orig.name
                AND orig.original
              LEFT OUTER JOIN stargazers old ON new.owner = old.owner
              AND new.name = old.name
            GROUP BY
              new.owner,
              new.name,
              new.stargazers
            HAVING
              new_count > old_count
            ",
    )?;
    let mut rows = stmt.query([])?;
    let mut result = Vec::new();
    while let Some(row) = rows.next()? {
        let owner = row.get(0)?;
        let name = row.get(1)?;
        let new_count = row.get(2)?;
        let old_count = row.get(3)?;
        result.push(StarCountDiff {
            owner,
            name,
            new_count,
            old_count,
        })
    }
    Ok(result)
}

pub fn insert_repository_primary_language(
    conn: &Connection,
    owner: &str,
    name: &str,
    primary_language: &Option<String>,
) -> anyhow::Result<()> {
    let mut app = conn.appender("repository_primary_languages")?;

    app.append_row(params![owner, name, primary_language])?;

    Ok(())
}

pub struct StargazerEntry {
    pub login: String,
    pub starred_at: String,
}

pub fn insert_stargazers(
    conn: &mut Connection,
    owner: &str,
    name: &str,
    items: Vec<StargazerEntry>,
) -> anyhow::Result<()> {
    // The records are inserted in descending order, so it's safer to use a
    // transaction.
    let mut tx = conn.transaction()?;
    tx.set_drop_behavior(DropBehavior::Commit);
    let mut app = tx.appender("stargazers")?;

    for item in items {
        let login = item.login;
        app.append_row(params![owner, name, item.starred_at, login])?;
    }

    Ok(())
}

pub fn collect_star_history_by_language(
    conn: &mut Connection,
    min_stargazer_count: i64,
) -> anyhow::Result<Vec<(NaiveDate, String, i64)>> {
    let mut stmt = conn.prepare(
        "WITH activities AS (
              SELECT
                l.primary_language AS
                LANGUAGE,
                s.date,
                count(s.idx) AS count
              FROM
                repository_primary_languages l
                INNER JOIN (
                SELECT
                  row_number() OVER () AS idx,
                  strftime(starred_at, '%Y-%m-%d') AS date,
                  OWNER,
                  name
                FROM
                  stargazers) s ON l.owner = s.owner
                  AND l.name = s.name
              WHERE
                l.primary_language IS NOT NULL
                AND l.primary_language IN (
                  SELECT
                    primary_language
                  FROM
                    total_stars_by_language_2
                  WHERE
                    stargazers >= $1)
                GROUP BY
                  l.primary_language,
                  s.date
            )
            SELECT
              date,
              LANGUAGE,
              sum(count) OVER (PARTITION BY
              LANGUAGE ORDER BY date ROWS UNBOUNDED PRECEDING) accum
            FROM
              activities
            ORDER BY
              date
            ",
    )?;

    let mut rows = stmt.query(params![min_stargazer_count])?;

    let mut vec = Vec::new();

    while let Some(row) = rows.next()? {
        let date_str: String = row.get(0)?;
        let date = NaiveDate::parse_from_str(date_str.as_str(), "%Y-%m-%d")?;
        let language = row.get(1)?;
        let accum = row.get(2)?;
        vec.push((date, language, accum));
    }

    Ok(vec)
}

pub fn collect_total_star_history(conn: &mut Connection) -> anyhow::Result<Vec<(NaiveDate, i64)>> {
    let mut stmt = conn.prepare(
        "WITH cte AS (
              SELECT
                row_number() OVER () AS idx,
                strftime (starred_at, '%Y-%m-%d') AS date
              FROM
                stargazers
            )
            SELECT
              date,
              count(idx) OVER (ORDER BY date ROWS UNBOUNDED PRECEDING) accum
              FROM
                cte
              ORDER BY
                date
            ",
    )?;

    let mut rows = stmt.query([])?;

    let mut vec = Vec::new();

    while let Some(row) = rows.next()? {
        let date_str: String = row.get(0)?;
        let date = NaiveDate::parse_from_str(date_str.as_str(), "%Y-%m-%d")?;
        let accum = row.get(1)?;
        vec.push((date, accum));
    }

    Ok(vec)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        setup(&conn);
        conn
    }

    #[test]
    fn test_insert_star_counts() {
        let mut conn = setup_test_db();
        let test_repos = vec![StarCountEntry {
            owner: "test_owner",
            name: "test_repo",
            stargazer_count: 42,
        }];

        assert!(insert_star_counts(&mut conn, &test_repos).is_ok());
        assert!(update_star_count(&mut conn, "test_owner", "test_repo", 43).is_ok());
    }

    #[test]
    fn test_insert_stargazers() {
        let mut conn = setup_test_db();

        assert!(insert_stargazers(
            &mut conn,
            "test_owner",
            "test_repo",
            vec![StargazerEntry {
                login: format!("test_another_user"),
                starred_at: format!("2024-09-21T11:08:01Z")
            }]
        )
        .is_ok());
    }

    #[test]
    fn test_insert_repository_primary_language() {
        let mut conn = setup_test_db();
        assert!(insert_repository_primary_language(
            &mut conn,
            "test_owner",
            "test_repo",
            &Some(format!("Rust"))
        )
        .is_ok());
    }

    #[test]
    fn test_original_status_operations() {
        let conn = setup_test_db();

        // Test inserting original status
        assert!(insert_original_status(&conn, "test_owner", "test_repo", true).is_ok());

        // Test retrieving original status keys
        let keys = original_status_keys(&conn).unwrap();
        assert!(keys.contains(&("test_owner".to_string(), "test_repo".to_string())));
    }

    #[test]
    fn test_star_history_by_language() {
        let mut conn = setup_test_db();

        let date1 = "2024-09-21T11:08:01Z";
        let date2 = "2024-09-22T11:08:01Z";
        let date3 = "2024-09-23T11:08:01Z";
        let date4 = "2024-09-24T11:08:01Z";

        let data = vec![
            (
                "test_repo1",
                "Rust",
                6,
                vec![date1, date1, date1, date2, date2, date2],
            ),
            (
                "test_repo2",
                "Rust",
                5,
                vec![date2, date2, date2, date3, date3],
            ),
            ("test_repo3", "C++", 4, vec![date1, date2, date3, date4]),
        ];

        insert_star_counts(
            &mut conn,
            &data
                .iter()
                .map(|(name, _, stargazer_count, _)| StarCountEntry {
                    owner: "test_owner",
                    name,
                    stargazer_count: *stargazer_count,
                })
                .collect(),
        )
        .unwrap();

        for (name, language, _, _) in data.iter() {
            insert_original_status(&conn, "test_owner", name, true).unwrap();
            insert_repository_primary_language(
                &mut conn,
                "test_owner",
                name,
                &Some(language.to_string()),
            )
            .unwrap();
        }

        for (name, _, _, dates) in data.iter() {
            insert_stargazers(
                &mut conn,
                "test_owner",
                name,
                dates
                    .iter()
                    .map(|date| StargazerEntry {
                        login: format!("test_another_user"),
                        starred_at: date.to_string(),
                    })
                    .collect(),
            )
            .unwrap();
        }

        let result = collect_star_history_by_language(&mut conn, 10);

        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 3);
    }

    #[test]
    fn test_get_newly_starred_original_repositories() {
        let conn = setup_test_db();
        let result = get_newly_starred_original_repositories(&conn).unwrap();
        assert!(result.is_empty()); // Initially empty
    }
}
