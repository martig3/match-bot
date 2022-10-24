use crate::commands::maps::Map;
use crate::commands::matches::SeriesType::{Bo1, Bo3, Bo5};
use crate::commands::matches::VoteType::Veto;
use crate::commands::setup::NewVoteInfo;
use crate::commands::team::Team;
use crate::Context;
use anyhow::Result;
use poise::command;
use serde::{Deserialize, Serialize};
use sqlx::types::time::OffsetDateTime;
use sqlx::{FromRow, Type};
use sqlx::{PgExecutor, PgPool};
use std::fmt;
use std::str::FromStr;

#[allow(unused)]
#[derive(Debug, FromRow)]
pub struct Match {
    id: i32,
    match_series: i32,
    map: i32,
    picked_by: i32,
    start_ct_team: i32,
    start_t_team: i32,
    completed_at: Option<OffsetDateTime>,
}

#[derive(Debug, Clone)]
pub struct NewMatch {
    pub(crate) map: i32,
    pub(crate) picked_by: i64,
    pub(crate) start_ct_team: Option<i64>,
    pub(crate) start_t_team: Option<i64>,
}

#[allow(unused)]
#[derive(Debug, FromRow)]
pub struct Server {
    pub id: i32,
    pub match_series: i32,
    pub hostname: String,
    pub game_port: i32,
    pub gotv_port: i32,
}

impl Server {
    async fn get_live(executor: impl PgExecutor<'_>) -> Result<Vec<Server>> {
        Ok(sqlx::query_as!(
            Server,
            "select s.* from servers s \
                join match_series ms on ms.id = s.match_series \
            where ms.completed_at is null",
        )
        .fetch_all(executor)
        .await?)
    }
    async fn get_by_series(executor: impl PgExecutor<'_>, match_series: i32) -> Result<Server> {
        Ok(sqlx::query_as!(
            Server,
            "select * from servers where match_series = $1",
            match_series
        )
        .fetch_one(executor)
        .await?)
    }
}

#[allow(unused)]
#[derive(Debug, FromRow)]
pub struct MatchSeries {
    pub id: i32,
    pub team_one: i64,
    pub team_two: i64,
    pub series_type: SeriesType,
    pub created_at: OffsetDateTime,
    pub completed_at: Option<OffsetDateTime>,
}

#[derive(Debug, FromRow)]
pub struct VoteInfo {
    pub id: i32,
    pub match_series: i32,
    pub map: i32,
    pub step_type: VoteType,
    pub team: i64,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize, Serialize, Type,
)]
#[sqlx(rename_all = "lowercase", type_name = "series_type")]
pub enum SeriesType {
    Bo1,
    Bo3,
    Bo5,
}

impl fmt::Display for SeriesType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Bo1 => write!(f, "bo1"),
            Bo3 => write!(f, "bo3"),
            Bo5 => write!(f, "bo5"),
        }
    }
}

impl FromStr for SeriesType {
    type Err = ();
    fn from_str(input: &str) -> Result<SeriesType, Self::Err> {
        match input {
            "bo1" => Ok(Bo1),
            "bo3" => Ok(Bo3),
            "bo5" => Ok(Bo5),
            _ => Err(()),
        }
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize, Serialize, Type,
)]
#[sqlx(rename_all = "lowercase", type_name = "vote_type")]
pub enum VoteType {
    Veto,
    Pick,
}

#[allow(unused)]
impl fmt::Display for VoteType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Veto => write!(f, "Veto"),
            Pick => write!(f, "Pick"),
        }
    }
}

#[derive(FromRow)]
#[allow(unused)]
struct MatchScore {
    pub id: i32,
    pub match_id: i32,
    pub team_one_score: i32,
    pub team_two_score: i32,
}

impl MatchScore {
    async fn get_in_progress(executor: impl PgExecutor<'_>) -> Result<Vec<MatchScore>> {
        Ok(sqlx::query_as(
            "select * from match_scores
                    where in_progress = true
                 order by id",
        )
        .fetch_all(executor)
        .await?)
    }
}

impl VoteInfo {
    pub async fn add(executor: impl PgExecutor<'_>, new: &NewVoteInfo) -> Result<Vec<VoteInfo>> {
        Ok(sqlx::query_as(
            "
            insert into vote_info (match_series, map, type, team)
                    VALUES
                        ($1, $2, $3, $4)
                    RETURNING *",
        )
        .bind(new.match_series)
        .bind(new.map)
        .bind(new.step_type)
        .bind(new.team)
        .fetch_all(executor)
        .await?)
    }
    async fn get_by_match_series(
        executor: impl PgExecutor<'_>,
        match_series: i32,
    ) -> Result<Vec<VoteInfo>> {
        Ok(
            sqlx::query_as("select * from vote_info where match_series = $1 order by id")
                .bind(match_series)
                .fetch_all(executor)
                .await?,
        )
    }
}

impl MatchSeries {
    pub async fn create(
        executor: impl PgExecutor<'_>,
        team_one: i64,
        team_two: i64,
        series_type: SeriesType,
    ) -> Result<MatchSeries> {
        Ok(sqlx::query_as(
            "INSERT INTO match_series
                        (team_one, team_two, series_type, created_at)
                    VALUES
                        ($1, $2, $3, now())
                    RETURNING *",
        )
        .bind(team_one)
        .bind(team_two)
        .bind(series_type)
        .fetch_one(executor)
        .await?)
    }
    async fn get_all(
        executor: impl PgExecutor<'_>,
        limit: u64,
        completed: bool,
    ) -> Result<Vec<MatchSeries>> {
        let completed_clause = if completed { "is not null" } else { "is null" };
        Ok(sqlx::query_as(
            format!(
                "select * from match_series
                    where completed_at {}
                 order by id desc limit $1",
                completed_clause
            )
            .as_str(),
        )
        .bind(limit as i64)
        .fetch_all(executor)
        .await?)
    }

    pub async fn next_user_match(executor: impl PgExecutor<'_>, user: i64) -> Result<MatchSeries> {
        Ok(sqlx::query_as(
            "
                select ms.*
                from match_series ms
                    join teams t on t.role = ms.team_one or t.role = ms.team_two
                    join team_members tm on t.id = tm.team
                    join steam_ids si on si.discord = tm.member
                where si.discord = $1
                order by ms.id",
        )
        .bind(user)
        .fetch_one(executor)
        .await?)
    }

    async fn get_all_by_user(
        executor: impl PgExecutor<'_>,
        limit: u64,
        user: u64,
        completed: bool,
    ) -> Result<Vec<MatchSeries>> {
        let completed_clause = if completed { "is not null" } else { "is null" };
        Ok(sqlx::query_as(
            format!(
                "select ms.*
                    from match_series ms
                    join match m on ms.id = m.match_series
                    join teams t on (t.id = ms.team_one or ms.team_two = t.id)
                    join team_members tm on t.id = tm.team
                where tm.member = $2
                    and completed_at {}
                order by ms.id desc
                limit $1",
                completed_clause
            )
            .as_str(),
        )
        .bind(limit as i64)
        .bind(user as i64)
        .fetch_all(executor)
        .await?)
    }
    pub async fn delete(executor: impl PgExecutor<'_>, id: i32) -> Result<bool> {
        let result = sqlx::query!("DELETE FROM match_series where id = $1", id)
            .execute(executor)
            .await?;
        Ok(result.rows_affected() == 1)
    }

    pub async fn info_string(
        &self,
        pool: &PgPool,
        vote_info: Option<Vec<VoteInfo>>,
    ) -> Result<String> {
        let vote_info = if vote_info.is_none() {
            VoteInfo::get_by_match_series(pool, self.id).await?
        } else {
            vote_info.unwrap()
        };
        if vote_info.is_empty() {
            return Ok(String::from("This match has no veto info yet"));
        }
        let team_one = Team::get(pool, self.team_one).await.unwrap();
        let team_two = Team::get(pool, self.team_two).await.unwrap();
        let maps = Map::get_all(pool, false).await.unwrap();
        let mut info_string = String::from("```diff\n");
        let rows: String = vote_info
            .into_iter()
            .map(|v| {
                let mut row_str = String::new();
                let team_name = if self.team_one == team_one.role {
                    &team_one.name
                } else {
                    &team_two.name
                };
                let map_name = &maps.iter().find(|m| m.id == v.map).unwrap().name;
                if v.step_type == Veto {
                    row_str.push_str(format!("- {} banned {}\n", team_name, map_name,).as_str());
                } else {
                    row_str.push_str(format!("+ {} picked {}\n", team_name, map_name,).as_str());
                }
                row_str
            })
            .collect();
        info_string.push_str(rows.as_str());
        info_string.push_str("```");
        Ok(info_string)
    }
}

impl Match {
    pub async fn create(
        executor: impl PgExecutor<'_>,
        match_series: i32,
        new: &NewMatch,
    ) -> Result<Match> {
        Ok(sqlx::query_as(
            "INSERT INTO match 
                        (match_series, map, picked_by, start_ct_team, start_t_team)
                    VALUES
                        ($1, $2, $3, $4, $5)
                    RETURNING *",
        )
        .bind(match_series)
        .bind(new.map)
        .bind(new.picked_by)
        .bind(new.start_ct_team)
        .bind(new.start_t_team)
        .fetch_one(executor)
        .await?)
    }

    async fn get_in_progress(executor: impl PgExecutor<'_>) -> Result<Vec<MatchSeries>> {
        Ok(sqlx::query_as(
            "select m.*
                 from match m
                   inner join match_scores mi on m.id = mi.match_id
                 where mi.in_progress is true",
        )
        .fetch_all(executor)
        .await?)
    }
}

#[command(
    slash_command,
    guild_only,
    subcommands("scheduled", "inprogress", "completed")
)]
pub(crate) async fn matches(_context: Context<'_>) -> Result<()> {
    Ok(())
}

#[command(slash_command, guild_only, ephemeral)]
pub(crate) async fn scheduled(context: Context<'_>, all: bool) -> Result<()> {
    let pool = &context.data().pool;
    let matches = if all {
        MatchSeries::get_all(pool, 20, false).await?
    } else {
        MatchSeries::get_all_by_user(pool, 20, context.author().id.0, false).await?
    };
    if matches.is_empty() {
        context.say("No matches were found").await?;
        return Ok(());
    }
    let teams = Team::get_all(pool).await?;
    let match_info: String = matches
        .into_iter()
        .map(|m| {
            let mut s = String::new();
            let team_one_name = &teams.iter().find(|t| t.role == m.team_one).unwrap().name;
            let team_two_name = &teams.iter().find(|t| t.role == m.team_two).unwrap().name;
            s.push_str(format!("`id: {}` ", m.id).as_str());
            s.push_str(format!("{}", &team_one_name).as_str());
            s.push_str(" vs ");
            s.push_str(format!("{}", &team_two_name).as_str());
            s.push_str("\n");
            s
        })
        .collect();
    context.say(match_info).await?;
    Ok(())
}

#[command(slash_command, guild_only, ephemeral)]
pub(crate) async fn inprogress(context: Context<'_>) -> Result<()> {
    let pool = &context.data().pool;
    let info = MatchScore::get_in_progress(pool).await?;
    let matches = Match::get_in_progress(pool).await?;
    if matches.is_empty() || info.is_empty() {
        context.say("No matches in progress were found").await?;
        return Ok(());
    }
    let servers = Server::get_live(pool).await?;
    let match_info: String = matches
        .into_iter()
        .map(|m| {
            let mut s = String::new();
            let m_info = info.iter().find(|i| i.match_id == m.id).unwrap();
            let team_one_score = m_info.team_one_score;
            let team_two_score = m_info.team_two_score;
            let server = servers.iter().find(|s| s.match_series == m.id).unwrap();
            s.push_str(format!("`#{}` ", m.id).as_str());
            s.push_str(format!("<@&{}> `**{}**`", &m.team_one, team_one_score).as_str());
            s.push_str(" - ");
            s.push_str(format!("`**{}**` <@&{}>", team_two_score, m.team_two).as_str());
            s.push_str("\n - ");
            s.push_str(
                format!(
                    "GOTV: ||`connect {}:{}`||\n",
                    server.hostname, server.gotv_port
                )
                .as_str(),
            );
            s
        })
        .collect();
    context.say(match_info).await?;
    Ok(())
}

#[command(slash_command, guild_only, ephemeral)]
pub(crate) async fn completed(context: Context<'_>, all: bool) -> Result<()> {
    let pool = &context.data().pool;
    let matches = if all {
        MatchSeries::get_all(pool, 20, true).await?
    } else {
        MatchSeries::get_all_by_user(pool, 20, context.author().id.0, true).await?
    };
    if matches.is_empty() {
        context.say("No matches were found").await?;
        return Ok(());
    }
    let teams = Team::get_all(pool).await?;
    let match_info: String = matches
        .into_iter()
        .map(|m| {
            let mut s = String::new();
            let team_one_name = &teams.iter().find(|t| t.role == m.team_one).unwrap().name;
            let team_two_name = &teams.iter().find(|t| t.role == m.team_one).unwrap().name;
            s.push_str(format!("`id: {}` ", m.id).as_str());
            s.push_str(format!("{}", &team_one_name).as_str());
            s.push_str(" vs ");
            s.push_str(format!("{}", &team_two_name).as_str());
            s.push_str("\n");
            s
        })
        .collect();
    context.say(match_info).await?;
    Ok(())
}
