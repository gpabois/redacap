//! Outil `read_document` : extrait le contenu textuel d'un document externe
//! (PDF, ODT, DOCX, HTML ou texte brut), fourni soit par référence
//! (`document_id`, obtenu via `request_document`), soit par une URL directe.

use std::io::{Cursor, Read};
use std::sync::Arc;

use async_trait::async_trait;
use regex::Regex;
use serde::Deserialize;
use serde_json::Value;

use crate::{
    error::ToolError,
    ports::DocumentContentPort,
    tool::{Tool, ToolOutput},
};

#[derive(Deserialize)]
struct ReadDocumentArguments {
    document_id: Option<String>,
    url: Option<String>,
    grep: Option<String>,
    grep_context: Option<usize>,
    sed_range: Option<String>,
    tail: Option<usize>,
}

/// Format de document supporté par [`ReadDocumentTool`], déterminé à partir
/// du type MIME ou, à défaut, de l'extension du nom de fichier/URL.
enum DocumentFormat {
    Pdf,
    Odt,
    Docx,
    Html,
    PlainText,
}

impl DocumentFormat {
    fn detect(mime_type: &str, file_name: &str) -> Option<Self> {
        let mime_type = mime_type.to_ascii_lowercase();
        let file_name = file_name.to_ascii_lowercase();
        if mime_type.contains("pdf") || file_name.ends_with(".pdf") {
            Some(Self::Pdf)
        } else if mime_type.contains("opendocument.text") || file_name.ends_with(".odt") {
            Some(Self::Odt)
        } else if mime_type.contains("wordprocessingml.document") || file_name.ends_with(".docx") {
            Some(Self::Docx)
        } else if mime_type.contains("html")
            || file_name.ends_with(".html")
            || file_name.ends_with(".htm")
        {
            Some(Self::Html)
        } else if mime_type.starts_with("text/")
            || file_name.ends_with(".txt")
            || file_name.ends_with(".md")
        {
            Some(Self::PlainText)
        } else {
            None
        }
    }
}

/// Outil `read_document` : lit un document externe (PDF ou ODT) et en
/// extrait le texte, pour le rendre disponible à l'agent sans que
/// l'inspecteur ait à le recopier manuellement.
pub struct ReadDocumentTool {
    /// `None` si la session n'a pas de moyen de relire un document par
    /// identifiant (voir `agent::ports::DocumentContentPort`) : seul le
    /// paramètre `url` reste alors utilisable.
    document_content: Option<Arc<dyn DocumentContentPort>>,
    http_client: reqwest::Client,
}

impl ReadDocumentTool {
    #[must_use]
    pub fn new(document_content: Option<Arc<dyn DocumentContentPort>>) -> Self {
        Self {
            document_content,
            http_client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl Tool for ReadDocumentTool {
    fn name(&self) -> &str {
        "read_document"
    }

    fn description(&self) -> &str {
        "Extrait le texte d'un document externe au format PDF, ODT, DOCX, HTML ou texte brut, \
         fourni soit par `document_id` (référence obtenue via un appel précédent à \
         `request_document`), soit par `url` (lien direct vers le fichier). Pour éviter de \
         renvoyer un texte trop long, le \
         résultat peut être filtré avec au plus un des paramètres `grep`, `sed_range` ou `tail` \
         (chaque ligne renvoyée est précédée de son numéro dans le document, pour pouvoir \
         ensuite cibler une plage avec `sed_range`)."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "document_id": {
                    "type": "string",
                    "description": "Identifiant renvoyé par un appel précédent à `request_document`"
                },
                "url": {
                    "type": "string",
                    "description": "URL directe vers un fichier PDF ou ODT"
                },
                "grep": {
                    "type": "string",
                    "description": "Motif (expression régulière) : ne renvoie que les lignes correspondantes, numérotées ; les blocs non contigus sont séparés par une ligne « -- »"
                },
                "grep_context": {
                    "type": "integer",
                    "minimum": 0,
                    "description": "Nombre de lignes de contexte à inclure avant/après chaque ligne correspondant à `grep` (par défaut 0) ; utilisable uniquement avec `grep`"
                },
                "sed_range": {
                    "type": "string",
                    "description": "Plage de lignes à extraire, au format « DEBUT,FIN » (numéros 1-indexés, inclusifs), ex : « 10,25 »"
                },
                "tail": {
                    "type": "integer",
                    "minimum": 1,
                    "description": "Ne renvoie que les N dernières lignes du texte extrait, numérotées"
                }
            }
        })
    }

    async fn call(&self, arguments: Value) -> Result<ToolOutput, ToolError> {
        let args: ReadDocumentArguments = serde_json::from_value(arguments)
            .map_err(|error| ToolError::InvalidArguments(error.to_string()))?;

        let filter_count = [
            args.grep.is_some(),
            args.sed_range.is_some(),
            args.tail.is_some(),
        ]
        .into_iter()
        .filter(|present| *present)
        .count();
        if filter_count > 1 {
            return Err(ToolError::InvalidArguments(
                "fournir au plus un des paramètres grep, sed_range ou tail".to_string(),
            ));
        }
        if args.grep_context.is_some() && args.grep.is_none() {
            return Err(ToolError::InvalidArguments(
                "grep_context ne peut être utilisé qu'avec grep".to_string(),
            ));
        }

        let (bytes, mime_type, file_name) = match (args.document_id, args.url) {
            (Some(_), Some(_)) => {
                return Err(ToolError::InvalidArguments(
                    "fournir document_id ou url, pas les deux".to_string(),
                ));
            }
            (Some(document_id), None) => {
                let port = self.document_content.as_ref().ok_or_else(|| {
                    ToolError::Other(
                        "lecture de document par identifiant non disponible sur cette session"
                            .to_string(),
                    )
                })?;
                let content = port.fetch_content(&document_id).await?;
                (content.bytes, content.mime_type, content.file_name)
            }
            (None, Some(url)) => {
                let response = self.http_client.get(&url).send().await?;
                let mime_type = response
                    .headers()
                    .get(reqwest::header::CONTENT_TYPE)
                    .and_then(|value| value.to_str().ok())
                    .and_then(|value| value.split(';').next())
                    .unwrap_or_default()
                    .trim()
                    .to_string();
                let bytes = response.bytes().await?.to_vec();
                (bytes, mime_type, url)
            }
            (None, None) => {
                return Err(ToolError::InvalidArguments(
                    "fournir document_id ou url".to_string(),
                ));
            }
        };

        let format = DocumentFormat::detect(&mime_type, &file_name).ok_or_else(|| {
            ToolError::Other(format!(
                "format de document non supporté (type MIME : « {mime_type} », fichier : « \
                 {file_name} ») ; seuls PDF, ODT, DOCX, HTML et texte brut sont pris en charge"
            ))
        })?;

        let text = match format {
            DocumentFormat::Pdf => extract_pdf_text(&bytes).await?,
            DocumentFormat::Odt => extract_odt_text(&bytes)?,
            DocumentFormat::Docx => extract_docx_text(&bytes)?,
            DocumentFormat::Html => extract_html_text(&bytes)?,
            DocumentFormat::PlainText => extract_plain_text(&bytes)?,
        };

        let text = if let Some(pattern) = &args.grep {
            grep_text(&text, pattern, args.grep_context.unwrap_or(0))?
        } else if let Some(range) = &args.sed_range {
            sed_range_text(&text, range)?
        } else if let Some(count) = args.tail {
            tail_text(&text, count)
        } else {
            text
        };

        Ok(ToolOutput::new(text))
    }
}

/// Équivalent de `grep -n` (éventuellement `-C` si `context > 0`) : ne
/// renvoie que les lignes correspondant au motif, numérotées (1-indexé),
/// les blocs non contigus étant séparés par une ligne `--` comme le ferait
/// `grep`.
fn grep_text(text: &str, pattern: &str, context: usize) -> Result<String, ToolError> {
    let regex = Regex::new(pattern)
        .map_err(|error| ToolError::InvalidArguments(format!("motif grep invalide : {error}")))?;
    let lines: Vec<&str> = text.lines().collect();

    let matches: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| regex.is_match(line))
        .map(|(index, _)| index)
        .collect();
    if matches.is_empty() {
        return Ok("aucune ligne ne correspond au motif indiqué".to_string());
    }

    // Fusionne les plages de contexte qui se chevauchent ou se touchent, pour
    // n'afficher qu'un seul bloc (sans séparateur `--` superflu) là où deux
    // correspondances sont proches l'une de l'autre.
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    for index in matches {
        let start = index.saturating_sub(context);
        let end = (index + context).min(lines.len().saturating_sub(1));
        match ranges.last_mut() {
            Some(last) if start <= last.1 + 1 => last.1 = last.1.max(end),
            _ => ranges.push((start, end)),
        }
    }

    let mut output = String::new();
    for (block_index, (start, end)) in ranges.iter().enumerate() {
        if block_index > 0 {
            output.push_str("--\n");
        }
        for (line_index, line) in lines.iter().enumerate().take(*end + 1).skip(*start) {
            output.push_str(&format!("{}: {line}\n", line_index + 1));
        }
    }
    Ok(output)
}

/// Équivalent de `sed -n 'DEBUT,FINp'` : renvoie la plage de lignes
/// demandée (1-indexée, inclusive), numérotées.
fn sed_range_text(text: &str, range: &str) -> Result<String, ToolError> {
    let (start, end) = range
        .split_once(',')
        .ok_or_else(|| {
            ToolError::InvalidArguments(
                "sed_range doit être au format « DEBUT,FIN » (ex : « 10,25 »)".to_string(),
            )
        })
        .and_then(|(start, end)| {
            let start: usize = start.trim().parse().map_err(|_| {
                ToolError::InvalidArguments(format!(
                    "numéro de ligne de début invalide : « {start} »"
                ))
            })?;
            let end: usize = end.trim().parse().map_err(|_| {
                ToolError::InvalidArguments(format!("numéro de ligne de fin invalide : « {end} »"))
            })?;
            Ok((start, end))
        })?;

    if start == 0 {
        return Err(ToolError::InvalidArguments(
            "sed_range : les numéros de ligne sont indexés à partir de 1".to_string(),
        ));
    }
    if start > end {
        return Err(ToolError::InvalidArguments(
            "sed_range : la ligne de début doit être inférieure ou égale à la ligne de fin"
                .to_string(),
        ));
    }

    let lines: Vec<&str> = text.lines().collect();
    if start > lines.len() {
        return Err(ToolError::InvalidArguments(format!(
            "sed_range : le document ne contient que {} ligne(s)",
            lines.len()
        )));
    }
    let end = end.min(lines.len());

    let mut output = String::new();
    for (offset, line) in lines[(start - 1)..end].iter().enumerate() {
        output.push_str(&format!("{}: {line}\n", start + offset));
    }
    Ok(output)
}

/// Équivalent de `tail -n`, avec numérotation des lignes renvoyées
/// (position réelle dans le document, et non 1..N).
fn tail_text(text: &str, count: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.len().saturating_sub(count);

    let mut output = String::new();
    for (offset, line) in lines[start..].iter().enumerate() {
        output.push_str(&format!("{}: {line}\n", start + offset + 1));
    }
    output
}

/// Extrait le texte d'un PDF en déléguant à `pdftotext` (Poppler), invoqué
/// comme sous-processus (stdin/stdout, sans fichier temporaire) : plus
/// robuste que les bibliothèques Rust pures sur les PDF réels (polices
/// Type0/Identity-H sans CMap `ToUnicode` embarquée, notamment), qui
/// renvoient souvent un texte vide sur ce type de document.
async fn extract_pdf_text(bytes: &[u8]) -> Result<String, ToolError> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::process::Command;

    let mut child = Command::new("pdftotext")
        .args(["-", "-"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|error| ToolError::Other(format!("échec du lancement de pdftotext : {error}")))?;

    let mut stdin = child
        .stdin
        .take()
        .expect("stdin capturé à la création du processus");
    let mut stdout = child
        .stdout
        .take()
        .expect("stdout capturé à la création du processus");
    let mut stderr = child
        .stderr
        .take()
        .expect("stderr capturé à la création du processus");
    let input = bytes.to_vec();

    // Écriture et lecture concurrentes : indispensable pour les PDF de plus
    // de quelques dizaines de Ko, sous peine d'interblocage (le tube stdout
    // se remplit pendant qu'on écrit encore sur stdin).
    let (write_result, stdout_result, stderr_result) = tokio::join!(
        async move {
            let result = stdin.write_all(&input).await;
            drop(stdin);
            result
        },
        async move {
            let mut buf = Vec::new();
            stdout.read_to_end(&mut buf).await.map(|_| buf)
        },
        async move {
            let mut buf = Vec::new();
            stderr.read_to_end(&mut buf).await.map(|_| buf)
        },
    );

    write_result
        .map_err(|error| ToolError::Other(format!("échec d'écriture vers pdftotext : {error}")))?;
    let output = stdout_result.map_err(|error| {
        ToolError::Other(format!(
            "échec de lecture de la sortie de pdftotext : {error}"
        ))
    })?;
    let stderr_output = stderr_result.unwrap_or_default();

    let status = child.wait().await.map_err(|error| {
        ToolError::Other(format!("échec de l'exécution de pdftotext : {error}"))
    })?;

    if !status.success() {
        return Err(ToolError::Other(format!(
            "échec de l'extraction du texte PDF (pdftotext, {status}) : {}",
            String::from_utf8_lossy(&stderr_output).trim()
        )));
    }

    let text = String::from_utf8_lossy(&output).into_owned();

    // `pdftotext` réussit (code 0) même lorsqu'il ne trouve aucun texte à
    // extraire, notamment pour un PDF composé uniquement d'images scannées
    // (aucune couche de texte, par opposition à un texte présent mais mal
    // encodé) : sans cette vérification, l'outil renverrait silencieusement
    // une chaîne vide, laissant croire à tort que le document ne contient
    // aucun texte.
    if text.trim().is_empty() {
        return Err(ToolError::Other(
            "aucun texte n'a pu être extrait de ce PDF ; il s'agit probablement d'une image \
             scannée sans couche de texte (l'extraction de texte ne fonctionne pas sur ce type \
             de document, une reconnaissance optique de caractères serait nécessaire)"
                .to_string(),
        ));
    }

    Ok(text)
}

fn extract_odt_text(bytes: &[u8]) -> Result<String, ToolError> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|error| {
        ToolError::Other(format!("échec de lecture de l'archive ODT : {error}"))
    })?;
    let mut content_xml = String::new();
    archive
        .by_name("content.xml")
        .map_err(|error| ToolError::Other(format!("content.xml introuvable dans l'ODT : {error}")))?
        .read_to_string(&mut content_xml)
        .map_err(|error| ToolError::Other(format!("échec de lecture de content.xml : {error}")))?;

    extract_text_from_odt_xml(&content_xml)
}

/// Extrait le texte visible d'un `content.xml` ODT : les paragraphes,
/// titres, éléments de liste et lignes de tableau sont séparés par un saut
/// de ligne, les tabulations et sauts de ligne explicites
/// (`text:tab`/`text:line-break`) sont restitués tels quels — le reste de la
/// mise en forme (styles, numérotation...) est ignoré.
fn extract_text_from_odt_xml(xml: &str) -> Result<String, ToolError> {
    use quick_xml::events::Event;
    use quick_xml::reader::Reader;

    let mut reader = Reader::from_str(xml);
    let mut output = String::new();
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Text(text)) => {
                let decoded = text
                    .decode()
                    .map_err(|error| ToolError::Other(format!("XML ODT invalide : {error}")))?;
                let unescaped = quick_xml::escape::unescape(&decoded)
                    .map_err(|error| ToolError::Other(format!("XML ODT invalide : {error}")))?;
                output.push_str(&unescaped);
            }
            Ok(Event::Start(tag)) | Ok(Event::Empty(tag)) => match tag.local_name().as_ref() {
                b"tab" => output.push('\t'),
                b"line-break" => output.push('\n'),
                _ => {}
            },
            Ok(Event::End(tag)) => {
                if matches!(
                    tag.local_name().as_ref(),
                    b"p" | b"h" | b"list-item" | b"table-row"
                ) {
                    output.push('\n');
                }
            }
            Ok(Event::Eof) => break,
            Err(error) => {
                return Err(ToolError::Other(format!(
                    "échec du parsing XML ODT : {error}"
                )));
            }
            _ => {}
        }
        buf.clear();
    }

    Ok(output)
}

fn extract_docx_text(bytes: &[u8]) -> Result<String, ToolError> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|error| {
        ToolError::Other(format!("échec de lecture de l'archive DOCX : {error}"))
    })?;
    let mut document_xml = String::new();
    archive
        .by_name("word/document.xml")
        .map_err(|error| {
            ToolError::Other(format!(
                "word/document.xml introuvable dans le DOCX : {error}"
            ))
        })?
        .read_to_string(&mut document_xml)
        .map_err(|error| {
            ToolError::Other(format!("échec de lecture de word/document.xml : {error}"))
        })?;

    extract_text_from_docx_xml(&document_xml)
}

/// Extrait le texte visible d'un `word/document.xml` DOCX : seul le contenu
/// des balises `<w:t>` (véritables runs de texte) est retenu, les
/// paragraphes et lignes de tableau (`w:p`/`w:tr`) sont séparés par un saut
/// de ligne, `w:tab`/`w:br` restitués comme tabulation/saut de ligne — le
/// reste de la mise en forme (styles, sections...) est ignoré.
fn extract_text_from_docx_xml(xml: &str) -> Result<String, ToolError> {
    use quick_xml::events::Event;
    use quick_xml::reader::Reader;

    let mut reader = Reader::from_str(xml);
    let mut output = String::new();
    let mut buf = Vec::new();
    let mut in_run_text = false;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(tag)) if tag.local_name().as_ref() == b"t" => in_run_text = true,
            Ok(Event::Empty(tag)) => match tag.local_name().as_ref() {
                b"tab" => output.push('\t'),
                b"br" | b"cr" => output.push('\n'),
                _ => {}
            },
            Ok(Event::Text(text)) if in_run_text => {
                let decoded = text
                    .decode()
                    .map_err(|error| ToolError::Other(format!("XML DOCX invalide : {error}")))?;
                let unescaped = quick_xml::escape::unescape(&decoded)
                    .map_err(|error| ToolError::Other(format!("XML DOCX invalide : {error}")))?;
                output.push_str(&unescaped);
            }
            Ok(Event::End(tag)) => match tag.local_name().as_ref() {
                b"t" => in_run_text = false,
                b"p" | b"tr" => output.push('\n'),
                _ => {}
            },
            Ok(Event::Eof) => break,
            Err(error) => {
                return Err(ToolError::Other(format!(
                    "échec du parsing XML DOCX : {error}"
                )));
            }
            _ => {}
        }
        buf.clear();
    }

    Ok(output)
}

/// Extrait le texte visible d'une page HTML : le contenu de `<script>` et
/// `<style>` est ignoré, les éléments de bloc usuels (paragraphes, titres,
/// listes, lignes de tableau, sauts de ligne) sont séparés par un saut de
/// ligne. Le nom des balises de fermeture n'est pas vérifié
/// (`check_end_names = false`), pour tolérer les balises auto-fermantes du
/// HTML qui ne sont pas fermées explicitement (`<br>`, `<meta>`...) ; les
/// entités mal formées sont conservées telles quelles plutôt que de faire
/// échouer l'extraction.
fn extract_text_from_html(html: &str) -> Result<String, ToolError> {
    use quick_xml::events::Event;
    use quick_xml::reader::Reader;

    // Le tokenizer de quick-xml échoue sur une référence d'entité mal formée
    // (un `&` non suivi d'un nom/code valide terminé par `;`), pourtant
    // fréquente en HTML « tag soup » : on l'échappe par avance plutôt que de
    // faire échouer toute l'extraction pour ce détail.
    let sanitized = sanitize_bare_ampersands(html);

    let mut reader = Reader::from_str(&sanitized);
    reader.config_mut().check_end_names = false;
    let mut output = String::new();
    let mut buf = Vec::new();
    let mut skip_depth = 0usize;

    const BLOCK_TAGS: &[&[u8]] = &[
        b"p", b"div", b"h1", b"h2", b"h3", b"h4", b"h5", b"h6", b"li", b"tr",
    ];

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(tag)) => {
                let name = tag.local_name().as_ref().to_ascii_lowercase();
                if name == b"script" || name == b"style" {
                    skip_depth += 1;
                } else if skip_depth == 0 && name == b"br" {
                    output.push('\n');
                }
            }
            Ok(Event::Empty(tag)) => {
                let name = tag.local_name().as_ref().to_ascii_lowercase();
                if skip_depth == 0 && name == b"br" {
                    output.push('\n');
                }
            }
            Ok(Event::Text(text)) if skip_depth == 0 => {
                let decoded = text
                    .decode()
                    .map_err(|error| ToolError::Other(format!("HTML invalide : {error}")))?;
                match quick_xml::escape::unescape(&decoded) {
                    Ok(unescaped) => output.push_str(&unescaped),
                    Err(_) => output.push_str(&decoded),
                }
            }
            // quick-xml tokenise chaque référence d'entité (`&amp;`, `&#233;`...)
            // comme un événement séparé, distinct du texte qui l'entoure.
            Ok(Event::GeneralRef(reference)) if skip_depth == 0 => {
                let decoded = reference
                    .decode()
                    .map_err(|error| ToolError::Other(format!("HTML invalide : {error}")))?;
                match reference.resolve_char_ref().ok().flatten() {
                    Some(character) => output.push(character),
                    None => match decoded.as_ref() {
                        "amp" => output.push('&'),
                        "lt" => output.push('<'),
                        "gt" => output.push('>'),
                        "quot" => output.push('"'),
                        "apos" => output.push('\''),
                        // Entité HTML (`&nbsp;`, `&copy;`...) non reconnue par
                        // les 5 entités prédéfinies XML : conservée telle
                        // quelle plutôt que perdue silencieusement.
                        other => {
                            output.push('&');
                            output.push_str(other);
                            output.push(';');
                        }
                    },
                }
            }
            Ok(Event::End(tag)) => {
                let name = tag.local_name().as_ref().to_ascii_lowercase();
                if name == b"script" || name == b"style" {
                    skip_depth = skip_depth.saturating_sub(1);
                } else if skip_depth == 0 && BLOCK_TAGS.contains(&name.as_slice()) {
                    output.push('\n');
                }
            }
            Ok(Event::Eof) => break,
            Err(error) => {
                return Err(ToolError::Other(format!("échec du parsing HTML : {error}")));
            }
            _ => {}
        }
        buf.clear();
    }

    Ok(output)
}

/// Échappe (`&amp;`) tout `&` qui n'introduit pas une référence d'entité
/// valide (`&nom;`, `&#123;` ou `&#x1F;`), pour que le tokenizer XML de
/// quick-xml ne rejette pas un `&` isolé, courant dans du HTML réel non
/// généré par un outil strict.
fn sanitize_bare_ampersands(html: &str) -> String {
    let chars: Vec<char> = html.chars().collect();
    let mut output = String::with_capacity(html.len());
    let mut index = 0;
    while index < chars.len() {
        if chars[index] == '&' {
            match valid_entity_end(&chars, index) {
                Some(end) => {
                    output.extend(&chars[index..=end]);
                    index = end + 1;
                }
                None => {
                    output.push_str("&amp;");
                    index += 1;
                }
            }
        } else {
            output.push(chars[index]);
            index += 1;
        }
    }
    output
}

/// Renvoie l'indice (dans `chars`) du `;` terminant une référence d'entité
/// valide qui commence en `start` (qui doit pointer sur `&`), ou `None` si
/// ce qui suit `&` n'en forme pas une.
fn valid_entity_end(chars: &[char], start: usize) -> Option<usize> {
    let mut index = start + 1;
    if chars.get(index) == Some(&'#') {
        index += 1;
        let is_hex = matches!(chars.get(index), Some('x') | Some('X'));
        if is_hex {
            index += 1;
        }
        let digits_start = index;
        while chars.get(index).is_some_and(|character| {
            if is_hex {
                character.is_ascii_hexdigit()
            } else {
                character.is_ascii_digit()
            }
        }) {
            index += 1;
        }
        return (index > digits_start && chars.get(index) == Some(&';')).then_some(index);
    }

    let name_start = index;
    while chars.get(index).is_some_and(char::is_ascii_alphanumeric) && index - name_start < 32 {
        index += 1;
    }
    (index > name_start && chars.get(index) == Some(&';')).then_some(index)
}

/// Décode un document texte brut (`.txt`, `.md`...) : les octets invalides
/// en UTF-8 sont remplacés plutôt que de faire échouer l'extraction, la
/// plupart des documents texte récupérés par URL n'ayant pas d'encodage
/// garanti.
fn extract_plain_text(bytes: &[u8]) -> Result<String, ToolError> {
    Ok(String::from_utf8_lossy(bytes).into_owned())
}

fn extract_html_text(bytes: &[u8]) -> Result<String, ToolError> {
    extract_text_from_html(&String::from_utf8_lossy(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// PDF minimal (un seul mot, police Times-Roman non embarquée) : la
    /// table `xref` indique volontairement des décalages incorrects, pour
    /// vérifier que `pdftotext` (mode de récupération) s'en accommode comme
    /// le ferait un vrai PDF légèrement corrompu.
    const MINIMAL_PDF: &[u8] = b"%PDF-1.1
1 0 obj  << /Type /Catalog /Pages 2 0 R >> endobj
2 0 obj  << /Type /Pages /Kids [3 0 R] /Count 1 >> endobj
3 0 obj  << /Type /Page /Parent 2 0 R /Resources << /Font << /F1 4 0 R >> >> /MediaBox [0 0 300 144] /Contents 5 0 R >> endobj
4 0 obj  << /Type /Font /Subtype /Type1 /BaseFont /Times-Roman >> endobj
5 0 obj  << /Length 73 >>
stream
  BT
    /F1 18 Tf
    0 0 Td
    (Bonjour le monde) Tj
  ET
endstream
endobj
xref
0 6
0000000000 65535 f
0000000018 00000 n
0000000077 00000 n
0000000178 00000 n
0000000457 00000 n
0000000549 00000 n
trailer
  <<  /Root 1 0 R
      /Size 6
  >>
startxref
625
%%EOF
";

    #[tokio::test]
    async fn extracts_text_from_a_pdf_via_pdftotext() {
        let text = extract_pdf_text(MINIMAL_PDF).await.unwrap();
        assert_eq!(text.trim(), "Bonjour le monde");
    }

    #[tokio::test]
    async fn reports_an_error_for_bytes_that_are_not_a_pdf() {
        let error = extract_pdf_text(b"pas un pdf").await.unwrap_err();
        assert!(matches!(error, ToolError::Other(_)));
    }

    /// PDF valide mais sans aucun contenu textuel (page blanche) : simule un
    /// PDF composé uniquement d'images scannées, pour lequel `pdftotext`
    /// réussit (code 0) sans rien extraire.
    const TEXTLESS_PDF: &[u8] = b"%PDF-1.1
1 0 obj  << /Type /Catalog /Pages 2 0 R >> endobj
2 0 obj  << /Type /Pages /Kids [3 0 R] /Count 1 >> endobj
3 0 obj  << /Type /Page /Parent 2 0 R /Resources << >> /MediaBox [0 0 300 144] /Contents 4 0 R >> endobj
4 0 obj  << /Length 0 >>
stream
endstream
endobj
trailer
  <<  /Root 1 0 R
      /Size 5
  >>
%%EOF
";

    #[tokio::test]
    async fn reports_an_error_when_the_pdf_has_no_extractable_text() {
        let error = extract_pdf_text(TEXTLESS_PDF).await.unwrap_err();
        assert!(matches!(error, ToolError::Other(_)));
    }

    #[test]
    fn extracts_paragraphs_separated_by_newlines() {
        // Comme un vrai `content.xml` ODT (généré sans espace insignifiant
        // entre les balises), pour ne pas capturer d'indentation comme texte.
        let xml = "<office:document-content \
            xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" \
            xmlns:text=\"urn:oasis:names:tc:opendocument:xmlns:text:1.0\">\
            <office:body><office:text>\
            <text:p>Premier paragraphe.</text:p>\
            <text:p>Second paragraphe avec accents : é à ç.</text:p>\
            </office:text></office:body></office:document-content>";

        let text = extract_text_from_odt_xml(xml).unwrap();

        assert_eq!(
            text,
            "Premier paragraphe.\nSecond paragraphe avec accents : é à ç.\n"
        );
    }

    #[test]
    fn detects_format_from_mime_type_or_extension() {
        assert!(matches!(
            DocumentFormat::detect("application/pdf", "rapport"),
            Some(DocumentFormat::Pdf)
        ));
        assert!(matches!(
            DocumentFormat::detect("", "rapport.odt"),
            Some(DocumentFormat::Odt)
        ));
        assert!(matches!(
            DocumentFormat::detect(
                "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
                "rapport"
            ),
            Some(DocumentFormat::Docx)
        ));
        assert!(matches!(
            DocumentFormat::detect("", "rapport.docx"),
            Some(DocumentFormat::Docx)
        ));
        assert!(matches!(
            DocumentFormat::detect("text/html; charset=utf-8", "page"),
            Some(DocumentFormat::Html)
        ));
        assert!(matches!(
            DocumentFormat::detect("", "notes.txt"),
            Some(DocumentFormat::PlainText)
        ));
        assert!(DocumentFormat::detect("image/png", "logo.png").is_none());
    }

    const FIVE_LINES: &str = "un\ndeux\ntrois\nquatre\ncinq";

    #[test]
    fn grep_returns_only_matching_lines_numbered() {
        let output = grep_text(FIVE_LINES, "^t", 0).unwrap();
        assert_eq!(output, "3: trois\n");
    }

    #[test]
    fn grep_includes_context_lines_and_merges_adjacent_blocks() {
        // Les correspondances (lignes 1 et 3) sont à distance 2 avec un
        // contexte de 1 : leurs plages [1,2] et [2,4] se touchent et doivent
        // fusionner en un seul bloc, sans séparateur `--`.
        let output = grep_text(FIVE_LINES, "^(un|trois)$", 1).unwrap();
        assert_eq!(output, "1: un\n2: deux\n3: trois\n4: quatre\n");
    }

    #[test]
    fn grep_separates_non_contiguous_blocks_with_double_dash() {
        let output = grep_text(FIVE_LINES, "^(un|cinq)$", 0).unwrap();
        assert_eq!(output, "1: un\n--\n5: cinq\n");
    }

    #[test]
    fn grep_reports_when_nothing_matches() {
        let output = grep_text(FIVE_LINES, "xyz", 0).unwrap();
        assert_eq!(output, "aucune ligne ne correspond au motif indiqué");
    }

    #[test]
    fn grep_rejects_an_invalid_regex() {
        let error = grep_text(FIVE_LINES, "(", 0).unwrap_err();
        assert!(matches!(error, ToolError::InvalidArguments(_)));
    }

    #[test]
    fn sed_range_extracts_the_requested_lines_numbered() {
        let output = sed_range_text(FIVE_LINES, "2,4").unwrap();
        assert_eq!(output, "2: deux\n3: trois\n4: quatre\n");
    }

    #[test]
    fn sed_range_clamps_an_end_beyond_the_document_length() {
        let output = sed_range_text(FIVE_LINES, "4,100").unwrap();
        assert_eq!(output, "4: quatre\n5: cinq\n");
    }

    #[test]
    fn sed_range_rejects_a_start_past_the_end_of_the_document() {
        let error = sed_range_text(FIVE_LINES, "10,12").unwrap_err();
        assert!(matches!(error, ToolError::InvalidArguments(_)));
    }

    #[test]
    fn sed_range_rejects_start_greater_than_end() {
        let error = sed_range_text(FIVE_LINES, "4,2").unwrap_err();
        assert!(matches!(error, ToolError::InvalidArguments(_)));
    }

    #[test]
    fn sed_range_rejects_a_malformed_range() {
        let error = sed_range_text(FIVE_LINES, "abc").unwrap_err();
        assert!(matches!(error, ToolError::InvalidArguments(_)));
    }

    #[test]
    fn tail_returns_the_last_n_lines_with_real_line_numbers() {
        let output = tail_text(FIVE_LINES, 2);
        assert_eq!(output, "4: quatre\n5: cinq\n");
    }

    #[test]
    fn tail_returns_everything_when_n_exceeds_the_line_count() {
        let output = tail_text(FIVE_LINES, 100);
        assert_eq!(output, "1: un\n2: deux\n3: trois\n4: quatre\n5: cinq\n");
    }

    #[test]
    fn docx_extraction_keeps_only_run_text_separated_by_paragraphs() {
        // Comme un vrai `word/document.xml` : seul le texte dans <w:t> doit
        // être capturé, pas l'espace insignifiant entre les balises de mise
        // en forme (rPr, pPr...).
        let xml = "<w:document \
            xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">\
            <w:body>\
            <w:p><w:r><w:rPr/><w:t>Premier paragraphe.</w:t></w:r></w:p>\
            <w:p><w:r><w:t>Second</w:t></w:r><w:r><w:tab/><w:t>paragraphe.</w:t></w:r></w:p>\
            </w:body></w:document>";

        let text = extract_text_from_docx_xml(xml).unwrap();

        assert_eq!(text, "Premier paragraphe.\nSecond\tparagraphe.\n");
    }

    #[test]
    fn extracts_text_from_a_minimal_docx_archive() {
        let mut buffer = Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut buffer);
            writer
                .start_file::<_, ()>("word/document.xml", zip::write::FileOptions::default())
                .unwrap();
            use std::io::Write;
            writer
                .write_all(
                    b"<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">\
                      <w:body><w:p><w:r><w:t>Bonjour le monde</w:t></w:r></w:p></w:body></w:document>",
                )
                .unwrap();
            writer.finish().unwrap();
        }

        let text = extract_docx_text(buffer.get_ref()).unwrap();

        assert_eq!(text.trim(), "Bonjour le monde");
    }

    #[test]
    fn html_extraction_ignores_script_and_style_and_separates_blocks() {
        let html = "<html><head><style>p{color:red}</style>\
            <script>console.log('x')</script></head>\
            <body><p>Premier paragraphe.</p><p>Second<br>avec saut.</p></body></html>";

        let text = extract_text_from_html(html).unwrap();

        assert_eq!(text, "Premier paragraphe.\nSecond\navec saut.\n");
    }

    #[test]
    fn html_extraction_tolerates_unclosed_and_malformed_markup() {
        // Balises non fermées (<br>, <meta>) et entité mal formée (`&` seul) :
        // ne doit pas faire échouer l'extraction (check_end_names = false et
        // repli sur le texte brut en cas d'entité invalide).
        let html = "<meta><p>Prix: 10 & plus</p>";

        let text = extract_text_from_html(html).unwrap();

        assert_eq!(text, "Prix: 10 & plus\n");
    }

    #[test]
    fn plain_text_extraction_replaces_invalid_utf8_instead_of_failing() {
        let text = extract_plain_text(b"Bonjour \xFF\xFE le monde").unwrap();
        assert!(text.starts_with("Bonjour "));
        assert!(text.ends_with(" le monde"));
    }
}
