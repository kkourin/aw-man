use std::{collections::HashMap, path::{Component, Path, PathBuf}, time::Duration};

use cynic::{QueryBuilder, MutationBuilder, http::ReqwestExt};
use reqwest::Response;
use serde::Deserialize;
use tokio::{sync::mpsc, time::sleep};
use std::fs::canonicalize;

#[cynic::schema("suwayomi")]
mod schema {}

/*
query GetSourceByName($name: String!, $lang: String!) {
  sources(filter: {name: {equalTo: $name}, lang: {likeInsensitive: $lang}}, first: 1) {
    nodes {
      id
      name
    }
  }
}
*/


#[derive(cynic::QueryVariables, Debug)]
pub struct GetSourceByNameVariables<'a> {
    pub lang: &'a str,
    pub name: &'a str,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(graphql_type = "Query", variables = "GetSourceByNameVariables")]
pub struct GetSourceByName {
    #[arguments(filter: { lang: { likeInsensitive: $lang }, name: { equalTo: $name } }, first: 1)]
    pub sources: SourceNodeList,
}

#[derive(cynic::QueryFragment, Debug)]
pub struct SourceNodeList {
    pub nodes: Vec<SourceType>,
}

#[derive(cynic::QueryFragment, Debug)]
pub struct SourceType {
    pub id: LongString,
    pub name: String,
}

#[derive(cynic::Scalar, Debug, Clone)]
pub struct LongString(pub String);

#[derive(Deserialize, Debug)]
pub struct ResponseData {
    pub data: GetSourceByName,
}


/*
query GetMangaByTitleAndSource($title: String!, $sourceId: LongString) {
  mangas(condition: {title: $title, sourceId: $sourceId}, first: 1) {
    nodes {
      id
    }
  }
}

 */

#[derive(cynic::QueryVariables, Debug)]
pub struct GetMangaByTitleAndSourceVariables<'a> {
    pub source_id: LongString,
    pub title: &'a str,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(graphql_type = "Query", variables = "GetMangaByTitleAndSourceVariables")]
pub struct GetMangaByTitleAndSource {
    #[arguments(condition: { sourceId: $source_id, title: $title }, first: 1)]
    pub mangas: MangaNodeList,
}

#[derive(cynic::QueryFragment, Debug)]
pub struct MangaNodeList {
    pub nodes: Vec<MangaType>,
}

#[derive(cynic::QueryFragment, Debug, Clone)]
pub struct MangaType {
    pub id: i32,
}

/*
query GetChapter( $mangaId: Int) {
  chapters(condition: { mangaId: $mangaId}) {
    nodes {
      id
      scanlator
      name
    }
  }
}
 */

#[derive(cynic::QueryVariables, Debug)]
pub struct GetChapterVariables {
    pub manga_id: Option<i32>,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(graphql_type = "Query", variables = "GetChapterVariables")]
pub struct GetChapter {
    #[arguments(condition: { mangaId: $manga_id })]
    pub chapters: ChapterNodeList,
}

#[derive(cynic::QueryFragment, Debug)]
pub struct ChapterNodeList {
    pub nodes: Vec<ChapterType>,
}

#[derive(cynic::QueryFragment, Debug)]
pub struct ChapterType {
    pub id: i32,
    pub scanlator: Option<String>,
    pub name: String,
}

/*
mutation SetChapterRead($page: Int) {
  updateChapter(input: {patch: {isRead: true, lastPageRead: $page}}) {
    chapter {
      id
    }
  }
}
*/

#[derive(cynic::QueryVariables, Debug)]
pub struct SetChapterReadVariables {
    pub chapter_id: i32,
    pub is_read: bool,
    pub page: i32,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(graphql_type = "Mutation", variables = "SetChapterReadVariables")]
pub struct SetChapterRead {
    #[arguments(input: { id: $chapter_id, patch: { isRead: $is_read, lastPageRead: $page } })]
    pub update_chapter: Option<UpdateChapterPayload>,
}

#[derive(cynic::QueryFragment, Debug)]
pub struct UpdateChapterPayload {
    pub chapter: ChapterType,
}


fn build_valid_filename(orig_name: String) -> String {
    // 1. Trim dots and spaces from both ends
    // trim_matches takes a closure or a slice of characters
    let trimmed = orig_name.trim_matches(|c| c == '.' || c == ' ');

    // 2. Return "(invalid)" if empty
    if trimmed.is_empty() {
        return String::from("(invalid)");
    }

    // Helper logic for validity
    let is_valid = |c: char| -> bool {
        let cp = c as u32;
        // Control chars 0x00..0x1F and DEL (0x7F)
        if cp <= 0x1f || cp == 0x7f {
            return false;
        }
        // Disallowed punctuation
        !matches!(c, '"' | '*' | '/' | ':' | '<' | '>' | '?' | '\\' | '|')
    };

    // 3. Process characters: replace invalid with '_' and limit to 240
    trimmed
        .chars() // Iterates over Unicode scalar values
        .map(|c| if is_valid(c) { c } else { '_' })
        .take(240) // Limit to 240 characters
        .collect() // Collect into a new String
}


#[derive(Debug)]
enum ProgressCommand {
    OpenArchive { path: PathBuf },
    UpdatePage {
        path: PathBuf,
        last_page_read: u32,
        is_read: bool,
        immediate: bool,
    },
}

#[derive(Debug)]
pub struct SuwaManager {
    tx: mpsc::UnboundedSender<ProgressCommand>
}


impl SuwaManager {
    pub fn new(manga_root: PathBuf, graphql_url: String) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        let worker = ProgressWorker {
            client: reqwest::Client::new(),
            manga_root: canonicalize(&manga_root).unwrap_or(manga_root),
            graphql_url,
            rx
        };
        tokio::spawn(worker.run());
        Self { tx }
    }

    pub fn open_archive(&self, path: PathBuf) {
        let _ = self.tx.send(ProgressCommand::OpenArchive { path });
    }

    pub fn update_page(&self, path: PathBuf, last_page_read: u32, is_read: bool, immediate: bool) {
        let _ = self.tx.send(ProgressCommand::UpdatePage {
            path,
            last_page_read,
            is_read,
            immediate
        });
    }
}




#[derive(Debug)]
struct SuwaManga {
    source: String,
    lang: String,
    title: String,
    directory: PathBuf,
    file: String,
}

type MangaIdFromFileMap = HashMap<String, i32>;

fn make_manga_id_from_file_map(chapters: Vec<ChapterType>) -> MangaIdFromFileMap {
    let mut map = HashMap::new();

    for chapter in chapters {
        // it's chapter.scanalator + "_" + chapter.name  if there is scanalator
        // otherwise just chapter.name
        let mut filename = if let Some(scanner) = chapter.scanlator {
            format!("{}_{}", scanner, chapter.name)
        } else {
             chapter.name
        };
        filename = build_valid_filename(filename) + ".cbz";

        map.insert(filename, chapter.id);
    }

    map
}

#[derive(Debug)]
struct UpdateChapterArgs {
    chapter_id: i32,
    is_read: bool,
    page: u32,
}

struct ProgressWorker {
    client: reqwest::Client,
    manga_root: PathBuf,
    graphql_url: String,
    rx: mpsc::UnboundedReceiver<ProgressCommand>
}

impl ProgressWorker {
    fn process_path(&self, path: &PathBuf) -> Option<SuwaManga> {
        // let path = simplified(path);
        let path_buf = path.strip_prefix(&self.manga_root).ok()?;

        let mut comps = path_buf.components();

        let source_and_lang = comps.next()?;
        let manga = comps.next()?;
        let chapter_file = comps.next()?.as_os_str().to_str()?;

        if comps.next().is_some() {
            return None;  // More than 3 components remaining
        }

        if !chapter_file.ends_with(".cbz") {
            return None;  // Not a .cbz file
        }

        // Parse source_and_lang into source name and language. It's always of the form "NAME (LANG)".
        // So parse out the last " (" and the trailing ")".
        let source_and_lang_str = source_and_lang.as_os_str().to_str()?;
        let open_paren_index = source_and_lang_str.rfind(" (")?;
        let source = source_and_lang_str[..open_paren_index].to_string();
        let lang = source_and_lang_str[open_paren_index + 2..source_and_lang_str.len() - 1].to_string();

        let manga_str = manga.as_os_str().to_str()?;

        let mut directory = PathBuf::from(source_and_lang.as_os_str());
        directory.push(manga.as_os_str());

        Some(SuwaManga {
            source,
            lang,
            title: manga_str.to_string(),
            directory: directory,
            file: chapter_file.to_string()
        })
    }


    async fn get_first_source_id_by_name(&self, name: &str, lang: &str) -> Option<LongString> {
        let operation = GetSourceByName::build(GetSourceByNameVariables { name, lang });
        let resp = self.client
            .post(&self.graphql_url)
            .run_graphql(operation)
            .await.ok()?;
        let data = resp.data?;
        let b = data.sources.nodes.first()?;
        Some(b.id.clone())
    }

    async fn get_manga_by_title_and_source(
        &self,
        title: &str,
        source_id: &LongString,
    ) -> Option<MangaType> {
        trace!("Getting manga by title: {}, source_id: {:?}", title, source_id);
        let operation = GetMangaByTitleAndSource::build(GetMangaByTitleAndSourceVariables {
            title,
            source_id: source_id.clone(),
        });
        trace!("Built operation: {:?}", operation);
        let resp = self.client
            .post(&self.graphql_url)
            .run_graphql(operation)
            .await.ok()?;
        trace!("Got response: {:?}", resp);
        let data = resp.data?;
        Some(data.mangas.nodes.first()?.clone())
    }

    async fn get_all_chapters_for_manga(&self, manga_id: i32) -> Option<Vec<ChapterType>> {
        let operation = GetChapter::build(GetChapterVariables {
            manga_id: Some(manga_id),
        });
        let resp = self.client
            .post(&self.graphql_url)
            .run_graphql(operation)
            .await.ok()?;
        let data = resp.data?;
        Some(data.chapters.nodes)
    }


    async fn update_chapter(&self, args: &UpdateChapterArgs) -> Option<()> {
        let operation = SetChapterRead::build(SetChapterReadVariables {
            chapter_id: args.chapter_id,
            is_read: args.is_read,
            page: args.page as i32,
        });
        let resp = self.client
            .post(&self.graphql_url)
            .run_graphql(operation)
            .await.ok()?;
        let data = resp.data?;
        Some(())
    }

    async fn run(mut self) {
        let mut cache: HashMap<PathBuf, MangaIdFromFileMap> = HashMap::new();

        let mut pending_update: Option<UpdateChapterArgs> = None;
        let sleep_timer = sleep(Duration::from_secs(86400 * 365));
        tokio::pin!(sleep_timer);
        let mut timer_active = false;
        const DEBOUNCE_SECONDS: u64 = 1;

        loop {
            tokio::select! {
                Some(cmd) = self.rx.recv() => {
                    match cmd {
                        ProgressCommand::OpenArchive { path } => {
                            let Some(manga) = self.process_path(&path) else { continue; };
                            trace!("Processed manga: {:?}", manga);
                            // If manga data already fetched and chapter in cache, skip. Otherwise update cache.
                            if let Some(manga_id_map) = cache.get(&manga.directory) {
                                if manga_id_map.contains_key(&manga.file) {
                                    continue;
                                }
                            }

                            let Some(source_id) = self.get_first_source_id_by_name(&manga.source, &manga.lang).await else { continue; };
                            // trace!("Opening archive for manga: {:?}, source ID: {:?}", manga, source_id);
                            let Some(manga_id) = self.get_manga_by_title_and_source(&manga.title, &source_id).await else { continue; };
                            // trace!("Manga ID: {:?}", manga_id);
                            let Some(chapters) = self.get_all_chapters_for_manga(manga_id.id).await else { continue; };
                            // trace!("Chapters: {:?}", chapters) ;
                            let manga_id_map = make_manga_id_from_file_map(chapters);
                            // trace!("Manga ID map: {:?}", manga_id_map);
                            cache.insert(manga.directory, manga_id_map);
                        }
                        ProgressCommand::UpdatePage { path, last_page_read, is_read, immediate } => {
                            trace!("Updating page {} at path: {:?}, immediate: {}", last_page_read, path, immediate);
                            let Some(manga) = self.process_path(&path) else { continue; };
                            // trace!("Processed manga for update: {:?}", manga);
                            let Some(manga_id_map) = cache.get(&manga.directory) else { continue; };
                            let Some(chapter_id) = manga_id_map.get(&manga.file) else { continue; };
                            // trace!("Updating chapter ID: {} to page: {}, is_read: {}, immediate: {}", chapter_id, last_page_read, is_read, immediate);
                            let args = UpdateChapterArgs {
                                chapter_id: *chapter_id,
                                is_read,
                                page: last_page_read
                            };
                            if immediate {
                                pending_update = None;
                                timer_active = false;

                                let _ = self.update_chapter(&args).await;
                                trace!("Applied update with args: {:?}", args);
                            } else {
                                // Defer for debounce
                                pending_update = Some(args);
                                timer_active = true;
                                sleep_timer.as_mut().reset(tokio::time::Instant::now() + Duration::from_secs(DEBOUNCE_SECONDS));
                            }

                        }
                    }
                }
                _ = &mut sleep_timer, if timer_active => {
                    if let Some(args) = pending_update.take() {
                        let _ = self.update_chapter(&args).await;
                        trace!("Applied debounced update for {:?}", args);
                    }
                    timer_active = false;
                }
            }
        }
    }
}

