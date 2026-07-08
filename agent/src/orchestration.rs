//! Moteur d'orchestration hiérarchique : un Superviseur (agent générique,
//! comme tout agent de ce module) délègue dynamiquement des sous-tâches à des
//! agents experts éphémères, instanciés à la volée depuis un
//! [`crate::catalog::AgentCatalog`] plutôt que codés en dur (voir
//! `agent::tools::DelegateToExpertTool`).
//!
//! Contrairement à l'ancienne boucle plate (`Agent::run`), [`Orchestrator`]
//! ne bloque jamais en attendant une réponse humaine : quand un outil
//! d'interaction (`ask_user`, `ask_questions`, `request_document`) ou une
//! confirmation est nécessaire, [`Orchestrator::drive`] s'arrête et renvoie
//! [`RunOutcome::Paused`] avec tout l'état nécessaire déjà consigné dans
//! [`OrchestrationRun`] (sérialisable). L'application hôte persiste cet état
//! et appelle [`Orchestrator::resume`] quand la réponse arrive — possiblement
//! sur une tout autre connexion, après un redémarrage du serveur : rien dans
//! ce module ne dépend de la durée de vie d'une tâche async ou d'un channel.
//!
//! [`OrchestrationRun::stack`] représente la pile d'agents actifs : le
//! Superviseur en position 0, puis un [`AgentFrame`] par niveau de
//! délégation. Le frame en tête de pile est seul actif ; ses parents portent
//! chacun un [`PendingTurn`] de raison [`PauseReason::Delegating`], en
//! attente que leur enfant se termine pour reprendre leur propre tour là où
//! ils l'avaient laissé.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};

use crate::{
    catalog::{AgentCatalog, AgentProfile},
    error::{AgentError, ModelError, ToolError},
    model::{ChatMessage, LanguageModel, Role, StreamEvent, ToolCall, ToolDefinition},
    observer::AgentObserver,
    ports::{DocumentRef, QuestionAnswer},
    tool::{DelegateRequest, DelegateTarget, PauseRequest, ToolRegistry},
};

/// Profondeur maximale de la pile d'orchestration (Superviseur racine +
/// délégations imbriquées) : l'outil `spawn_expert` (voir
/// `agent::tools::SpawnExpertTool`) permet à un Superviseur imbriqué de se
/// redéléguer à lui-même de façon dynamique, sans profil ni catalogue pour
/// le borner naturellement ; cette limite fait échouer proprement une
/// délégation en trop plutôt que de laisser la pile croître sans limite.
const MAX_STACK_DEPTH: usize = 8;

/// Un frame de la pile d'orchestration : le Superviseur (`profile_id: None`)
/// ou un agent expert éphémère instancié depuis un [`AgentProfile`]. Sa
/// configuration (`system_prompt`/`tool_names`/`max_steps`) est figée à sa
/// création : une modification ultérieure du catalogue n'affecte jamais un
/// frame déjà en cours.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentFrame {
    pub agent_label: String,
    pub profile_id: Option<String>,
    pub system_prompt: String,
    pub tool_names: Vec<String>,
    pub max_steps: u32,
    pub history: Vec<ChatMessage>,
    pub steps_taken: u32,
    /// `Some` si ce frame est bloqué au milieu d'un tour : soit en attente
    /// d'une réponse humaine, soit en attente que le frame enfant qu'il vient
    /// de déléguer se termine.
    pub pending: Option<PendingTurn>,
}

impl AgentFrame {
    fn new(
        agent_label: String,
        profile_id: Option<String>,
        system_prompt: String,
        tool_names: Vec<String>,
        max_steps: u32,
        task: &str,
    ) -> Self {
        let mut history = Vec::new();
        if !system_prompt.trim().is_empty() {
            history.push(ChatMessage::system(system_prompt.clone()));
        }
        history.push(ChatMessage::user(task));
        Self {
            agent_label,
            profile_id,
            system_prompt,
            tool_names,
            max_steps,
            history,
            steps_taken: 0,
            pending: None,
        }
    }

    /// Frame racine d'une [`OrchestrationRun`] : le Superviseur.
    #[must_use]
    pub fn supervisor(
        system_prompt: impl Into<String>,
        tool_names: Vec<String>,
        max_steps: u32,
        task: &str,
    ) -> Self {
        Self::new(
            "Superviseur".to_string(),
            None,
            system_prompt.into(),
            tool_names,
            max_steps,
            task,
        )
    }

    fn from_profile(profile: &AgentProfile, task: &str) -> Self {
        Self::new(
            profile.display_name.clone(),
            Some(profile.id.clone()),
            profile.system_prompt.clone(),
            profile.tool_names.clone(),
            profile.max_steps,
            task,
        )
    }

    /// Frame d'un Superviseur imbriqué, instancié par l'outil `spawn_expert`
    /// (voir `agent::tools::SpawnExpertTool`) : reprend la configuration de
    /// `template` (prompt système, outils, budget de tours) — toujours le
    /// frame racine (`run.stack[0]`, seul frame garanti être le Superviseur)
    /// — pour que cette instance dispose des mêmes capacités de délégation
    /// que lui, mais démarre avec son propre historique et son propre
    /// compteur de tours, indépendants de `template`.
    #[must_use]
    fn nested_supervisor(template: &AgentFrame, task: &str) -> Self {
        Self::new(
            template.agent_label.clone(),
            None,
            template.system_prompt.clone(),
            template.tool_names.clone(),
            template.max_steps,
            task,
        )
    }

    /// Reprend la conversation d'un frame racine déjà terminé (voir
    /// [`RunStatus::Done`]) pour une nouvelle tâche sur la même salle,
    /// plutôt que de repartir d'une conversation vide : ajoute `task` comme
    /// nouveau message utilisateur à la suite de l'historique existant, et
    /// réinitialise le compteur de tours (`max_steps` s'applique par tâche,
    /// pas cumulativement sur toute la durée de la salle).
    #[must_use]
    pub fn resume_as_new_task(mut self, task: &str) -> Self {
        self.history.push(ChatMessage::user(task));
        self.steps_taken = 0;
        self.pending = None;
        self
    }
}

/// État d'un tour partiellement exécuté : certains appels d'outils du tour
/// courant ont déjà un résultat (`resolved`), un autre (`awaiting`) est en
/// attente — d'une réponse humaine ou de la fin d'un frame enfant délégué —
/// et le reste sera traité une fois celui-ci résolu.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingTurn {
    pub tool_calls: Vec<ToolCall>,
    pub resolved: HashMap<String, String>,
    pub awaiting: String,
    pub reason: PauseReason,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PauseReason {
    /// En attente d'une réponse humaine (voir [`PauseRequest`]).
    Interaction(PauseRequest),
    /// En attente que le frame enfant (empilé juste au-dessus) se termine —
    /// jamais visible de l'application hôte, purement interne.
    Delegating,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RunStatus {
    Running,
    Paused,
    Done,
    Failed,
}

/// État complet, sérialisable, d'une orchestration en cours. C'est la seule
/// donnée que l'application hôte doit persister pour qu'une pause survive à
/// une déconnexion ou un redémarrage : [`Orchestrator`] lui-même ne conserve
/// aucun état entre deux appels à [`Orchestrator::drive`]/[`Orchestrator::resume`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestrationRun {
    pub stack: Vec<AgentFrame>,
    pub status: RunStatus,
    pub final_answer: Option<String>,
}

impl OrchestrationRun {
    #[must_use]
    pub fn new(root: AgentFrame) -> Self {
        Self {
            stack: vec![root],
            status: RunStatus::Running,
            final_answer: None,
        }
    }
}

/// Résultat d'un appel à [`Orchestrator::drive`]/[`Orchestrator::resume`] qui
/// n'a pas échoué. Une erreur (`Result::Err`) laisse `run.status` inchangé :
/// c'est à l'appelant de le positionner à [`RunStatus::Failed`] avant de
/// persister, l'orchestrateur ne le fait pas lui-même pour ne pas préjuger de
/// ce que l'application hôte veut en faire (nouvelle tentative, etc.).
#[derive(Debug, Clone)]
pub enum RunOutcome {
    Done(String),
    Paused {
        agent_label: String,
        request: PauseRequest,
    },
}

/// Réponse humaine à une [`PauseRequest`] en attente, fournie à
/// [`Orchestrator::resume`]. La variante doit correspondre à la nature de la
/// pause en cours (voir [`Orchestrator::render_pause_answer`]) : une requête
/// [`PauseRequest::RequestDocument`] appelle une réponse de type
/// [`DocumentRef`] (construite par l'application hôte après avoir persisté
/// les octets du document uploadé), jamais le contenu brut du fichier.
#[derive(Debug, Clone)]
pub enum PauseAnswer {
    Text(String),
    Bool(bool),
    Questions(Vec<QuestionAnswer>),
    Document(DocumentRef),
}

/// Résultat de la résolution des appels d'outils d'un tour (voir
/// [`Orchestrator::resolve_turn`]).
enum TurnResolution {
    /// Tous les appels du tour ont un résultat.
    Completed {
        tool_calls: Vec<ToolCall>,
        resolved: HashMap<String, String>,
    },
    /// Un appel a déclenché une délégation : un nouveau frame a été empilé,
    /// `run` a déjà été mis à jour en conséquence.
    Delegated,
    /// Un appel nécessite une réponse humaine : `run` a déjà été mis à jour
    /// en conséquence.
    Paused(RunOutcome),
}

/// Accumulateur d'un appel d'outil en cours de réception depuis les
/// fragments successifs d'un [`StreamEvent::ToolCallDelta`] partageant le
/// même `index` (voir [`Orchestrator::run_turn`]).
#[derive(Default)]
struct PartialToolCall {
    id: String,
    name: String,
    arguments: String,
}

impl PartialToolCall {
    fn finish(self) -> Result<ToolCall, ModelError> {
        let arguments = serde_json::from_str(&self.arguments).map_err(|error| {
            ModelError::InvalidResponse(format!(
                "arguments d'appel d'outil invalides pour « {} » : {error}",
                self.name
            ))
        })?;
        Ok(ToolCall {
            id: self.id,
            name: self.name,
            arguments,
        })
    }
}

/// Concatène, dans leur ordre d'émission, le contenu textuel de tous les
/// tours assistant d'un historique de frame (séparés par une ligne vide) :
/// sert à reconstituer la réponse complète d'un frame éphémère (expert
/// délégué) qui aurait réparti sa synthèse entre plusieurs tours plutôt que
/// de tout dire dans son dernier — voir l'appelant dans
/// [`Orchestrator::drive`].
fn frame_answer_text(history: &[ChatMessage]) -> String {
    history
        .iter()
        .filter(|message| message.role == Role::Assistant)
        .filter_map(|message| message.content.as_deref())
        .filter(|content| !content.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// Orchestrateur hiérarchique : construit une fois par connexion/session
/// (voir `server::editor::ws`), il pilote autant de [`OrchestrationRun`]
/// qu'il en reçoit, sans conserver lui-même d'état entre deux appels.
pub struct Orchestrator {
    model: Arc<dyn LanguageModel>,
    /// Registre complet des outils disponibles pour cette session ; chaque
    /// frame n'en voit qu'un sous-ensemble (voir [`ToolRegistry::subset`]).
    tools: ToolRegistry,
    catalog: Arc<dyn AgentCatalog>,
    observer: Arc<dyn AgentObserver>,
    /// Partagé avec l'appelant : quand `true`, les outils marqués
    /// [`crate::tool::Tool::requires_confirmation`] s'exécutent sans passer
    /// par une confirmation humaine.
    auto_accept: Arc<AtomicBool>,
}

impl Orchestrator {
    #[must_use]
    pub fn new(
        model: Arc<dyn LanguageModel>,
        tools: ToolRegistry,
        catalog: Arc<dyn AgentCatalog>,
        observer: Arc<dyn AgentObserver>,
        auto_accept: Arc<AtomicBool>,
    ) -> Self {
        Self {
            model,
            tools,
            catalog,
            observer,
            auto_accept,
        }
    }

    /// Exécute `run` jusqu'à ce qu'il se termine ([`RunOutcome::Done`]) ou
    /// nécessite une réponse humaine ([`RunOutcome::Paused`]). Ne bloque
    /// jamais sur une E/S humaine : la seule attente possible est celle d'un
    /// appel au modèle ou à un outil (édition, API externe...), toujours
    /// bornée dans le temps.
    pub async fn drive(&self, run: &mut OrchestrationRun) -> Result<RunOutcome, AgentError> {
        loop {
            let Some(frame_index) = run.stack.len().checked_sub(1) else {
                run.status = RunStatus::Done;
                let answer = run.final_answer.clone().unwrap_or_default();
                return Ok(RunOutcome::Done(answer));
            };

            if let Some(pending) = run.stack[frame_index].pending.take() {
                let resolution = self
                    .resolve_turn(run, frame_index, pending.tool_calls, pending.resolved)
                    .await?;
                match self.apply_resolution(run, frame_index, resolution) {
                    Some(outcome) => return Ok(outcome),
                    None => continue,
                }
            }

            if run.stack[frame_index].steps_taken >= run.stack[frame_index].max_steps {
                return Err(AgentError::MaxStepsExceeded(
                    run.stack[frame_index].max_steps,
                ));
            }

            let agent_label = run.stack[frame_index].agent_label.clone();
            let tool_definitions = self
                .tools
                .subset(&run.stack[frame_index].tool_names)
                .definitions();
            let response = self
                .run_turn(
                    &agent_label,
                    &run.stack[frame_index].history,
                    &tool_definitions,
                )
                .await?;
            self.observer.on_turn_finished(&agent_label).await;
            run.stack[frame_index].steps_taken += 1;

            let tool_calls = response.tool_calls.clone();
            let final_content = response.content.clone();
            run.stack[frame_index].history.push(response);

            if tool_calls.is_empty() {
                if frame_index == 0 {
                    // Le Superviseur a terminé : contrairement à un frame
                    // expert, on ne le dépile pas — son historique complet
                    // reste dans `run.stack` pour qu'une tâche suivante sur
                    // la même salle puisse reprendre la conversation là où
                    // elle s'est arrêtée (voir `server::editor::ws`, qui
                    // réhydrate un nouveau run à partir de ce frame plutôt
                    // que de repartir d'une conversation vide).
                    let text = final_content.unwrap_or_default();
                    run.status = RunStatus::Done;
                    run.final_answer = Some(text.clone());
                    return Ok(RunOutcome::Done(text));
                }

                // Contrairement au Superviseur, un frame expert est
                // éphémère (une seule tâche, jamais repris) : son
                // historique complet ne couvre que la délégation en cours,
                // donc reprendre tous ses tours narrés plutôt que le seul
                // dernier ne risque pas de remonter du contenu d'une tâche
                // précédente. Nécessaire car un expert peut répartir sa
                // synthèse entre plusieurs tours (narration en marge
                // d'appels d'outils) avant de conclure par un dernier tour
                // parfois minimal (« Terminé. ») : se limiter à ce dernier
                // tour tronquait la réponse bubblée au Superviseur, alors
                // même que chaque tour restait visible séparément dans le
                // panneau (voir `agent::panel`).
                let text = frame_answer_text(&run.stack[frame_index].history);

                run.stack.pop();
                let parent_index = frame_index - 1;
                let parent_label = run.stack[parent_index].agent_label.clone();
                let awaiting = {
                    let pending = run.stack[parent_index].pending.as_mut().expect(
                        "un frame enfant ne se termine que si son parent est en attente de lui",
                    );
                    debug_assert!(matches!(pending.reason, PauseReason::Delegating));
                    pending
                        .resolved
                        .insert(pending.awaiting.clone(), text.clone());
                    pending.awaiting.clone()
                };
                self.observer
                    .on_tool_call_finished(&parent_label, &awaiting, &Ok(text))
                    .await;
                continue;
            }

            let resolution = self
                .resolve_turn(run, frame_index, tool_calls, HashMap::new())
                .await?;
            if let Some(outcome) = self.apply_resolution(run, frame_index, resolution) {
                return Ok(outcome);
            }
        }
    }

    /// Reprend un `run` en [`RunStatus::Paused`] avec la réponse humaine
    /// `answer` à l'interaction en attente du frame en tête de pile, puis
    /// relance [`Self::drive`]. Échoue si `run` n'est pas en attente d'une
    /// réponse humaine, ou si `answer` ne correspond pas à la nature de la
    /// question posée.
    pub async fn resume(
        &self,
        run: &mut OrchestrationRun,
        answer: PauseAnswer,
    ) -> Result<RunOutcome, AgentError> {
        let frame_index = run
            .stack
            .len()
            .checked_sub(1)
            .ok_or(AgentError::NotPaused)?;

        let (agent_label, awaiting, content) = {
            let frame = &mut run.stack[frame_index];
            let pending = frame.pending.as_mut().ok_or(AgentError::NotPaused)?;
            let PauseReason::Interaction(request) = &pending.reason else {
                return Err(AgentError::NotPaused);
            };
            let content = Self::render_pause_answer(request, &answer)?;
            pending
                .resolved
                .insert(pending.awaiting.clone(), content.clone());
            (frame.agent_label.clone(), pending.awaiting.clone(), content)
        };

        self.observer
            .on_tool_call_finished(&agent_label, &awaiting, &Ok(content))
            .await;
        run.status = RunStatus::Running;
        self.drive(run).await
    }

    /// Traduit `resolution` en effet sur la boucle de [`Self::drive`] :
    /// `None` signifie « reboucler », `Some` porte le résultat à renvoyer.
    fn apply_resolution(
        &self,
        run: &mut OrchestrationRun,
        frame_index: usize,
        resolution: TurnResolution,
    ) -> Option<RunOutcome> {
        match resolution {
            TurnResolution::Completed {
                tool_calls,
                resolved,
            } => {
                let frame = &mut run.stack[frame_index];
                for call in &tool_calls {
                    let content = resolved.get(&call.id).cloned().unwrap_or_default();
                    frame
                        .history
                        .push(ChatMessage::tool_result(call.id.clone(), content));
                }
                None
            }
            TurnResolution::Delegated => None,
            TurnResolution::Paused(outcome) => {
                run.status = RunStatus::Paused;
                Some(outcome)
            }
        }
    }

    /// Résout la cible d'une [`DelegateRequest`] en un nouveau frame à
    /// empiler : un profil nommé du catalogue (`delegate_to_expert`), ou une
    /// instance imbriquée du Superviseur qui choisit elle-même l'expert
    /// approprié (`spawn_expert`, voir [`AgentFrame::nested_supervisor`]).
    /// Refuse la délégation (message d'erreur, à faire remonter au modèle
    /// comme résultat d'outil plutôt que d'interrompre toute l'exécution) si
    /// la pile a déjà atteint [`MAX_STACK_DEPTH`] — ce qui borne notamment un
    /// Superviseur imbriqué qui se redéléguerait indéfiniment à lui-même via
    /// `spawn_expert`.
    async fn resolve_delegate_target(
        &self,
        run: &OrchestrationRun,
        request: &DelegateRequest,
    ) -> Result<AgentFrame, String> {
        if run.stack.len() >= MAX_STACK_DEPTH {
            return Err(format!(
                "profondeur maximale de délégation atteinte ({MAX_STACK_DEPTH}) : réponds \
                 directement avec ce que tu sais déjà plutôt que de déléguer à nouveau"
            ));
        }
        match &request.target {
            DelegateTarget::Profile(profile_id) => match self.catalog.get(profile_id).await {
                Ok(Some(profile)) => Ok(AgentFrame::from_profile(&profile, &request.task)),
                Ok(None) => Err(format!("expert inconnu : « {profile_id} »")),
                Err(error) => Err(error.to_string()),
            },
            DelegateTarget::Supervisor => {
                Ok(AgentFrame::nested_supervisor(&run.stack[0], &request.task))
            }
        }
    }

    /// Résout dans l'ordre les appels d'outils d'un tour, en sautant ceux
    /// déjà présents dans `resolved` (reprise après pause ou retour de
    /// délégation) : normaux (exécutés immédiatement), délégation (empile un
    /// nouveau frame et s'arrête là) ou interaction humaine (marque le frame
    /// en attente et s'arrête là).
    async fn resolve_turn(
        &self,
        run: &mut OrchestrationRun,
        frame_index: usize,
        tool_calls: Vec<ToolCall>,
        mut resolved: HashMap<String, String>,
    ) -> Result<TurnResolution, AgentError> {
        let tool_names = run.stack[frame_index].tool_names.clone();
        let tools = self.tools.subset(&tool_names);
        let agent_label = run.stack[frame_index].agent_label.clone();

        for index in 0..tool_calls.len() {
            let call = tool_calls[index].clone();
            if resolved.contains_key(&call.id) {
                continue;
            }
            self.observer
                .on_tool_call_started(&agent_label, &call)
                .await;

            let Some(tool) = tools.get(&call.name) else {
                let message = format!("outil inconnu : « {} »", call.name);
                self.observer
                    .on_tool_call_finished(&agent_label, &call.id, &Err(message.clone()))
                    .await;
                resolved.insert(call.id.clone(), format!("erreur : {message}"));
                continue;
            };

            match tool.delegate_request(&call.arguments) {
                Ok(Some(request)) => {
                    match self.resolve_delegate_target(run, &request).await {
                        Ok(child) => {
                            run.stack.push(child);
                            run.stack[frame_index].pending = Some(PendingTurn {
                                tool_calls: tool_calls.clone(),
                                resolved,
                                awaiting: call.id.clone(),
                                reason: PauseReason::Delegating,
                            });
                            return Ok(TurnResolution::Delegated);
                        }
                        Err(message) => {
                            self.observer
                                .on_tool_call_finished(
                                    &agent_label,
                                    &call.id,
                                    &Err(message.clone()),
                                )
                                .await;
                            resolved.insert(call.id.clone(), format!("erreur : {message}"));
                        }
                    }
                    continue;
                }
                Ok(None) => {}
                Err(error) => {
                    let message = error.to_string();
                    self.observer
                        .on_tool_call_finished(&agent_label, &call.id, &Err(message.clone()))
                        .await;
                    resolved.insert(call.id.clone(), format!("erreur : {message}"));
                    continue;
                }
            }

            let pause_request = match tool.pause_request(&call.arguments) {
                Ok(request) => request,
                Err(error) => {
                    let message = error.to_string();
                    self.observer
                        .on_tool_call_finished(&agent_label, &call.id, &Err(message.clone()))
                        .await;
                    resolved.insert(call.id.clone(), format!("erreur : {message}"));
                    continue;
                }
            };
            let pause_request = pause_request.or_else(|| {
                (tool.requires_confirmation() && !self.auto_accept.load(Ordering::Relaxed)).then(
                    || PauseRequest::Confirm {
                        message: format!(
                            "Autoriser l'outil « {} » avec les paramètres {} ?",
                            call.name, call.arguments
                        ),
                    },
                )
            });

            if let Some(request) = pause_request {
                run.stack[frame_index].pending = Some(PendingTurn {
                    tool_calls: tool_calls.clone(),
                    resolved,
                    awaiting: call.id.clone(),
                    reason: PauseReason::Interaction(request.clone()),
                });
                return Ok(TurnResolution::Paused(RunOutcome::Paused {
                    agent_label,
                    request,
                }));
            }

            let result = tool.call(call.arguments.clone()).await;
            let observed = result
                .as_ref()
                .map(|output| output.0.clone())
                .map_err(ToolError::to_string);
            self.observer
                .on_tool_call_finished(&agent_label, &call.id, &observed)
                .await;
            let content = match result {
                Ok(output) => output.0,
                Err(error) => format!("erreur : {error}"),
            };
            resolved.insert(call.id.clone(), content);
        }

        Ok(TurnResolution::Completed {
            tool_calls,
            resolved,
        })
    }

    /// Convertit une [`PauseAnswer`] en contenu de `tool_result`, en
    /// vérifiant qu'elle correspond bien à la nature de `request`.
    fn render_pause_answer(
        request: &PauseRequest,
        answer: &PauseAnswer,
    ) -> Result<String, AgentError> {
        match (request, answer) {
            (PauseRequest::Ask { .. }, PauseAnswer::Text(text)) => Ok(text.clone()),
            (PauseRequest::Confirm { .. }, PauseAnswer::Bool(confirmed)) => Ok(if *confirmed {
                "confirmé".to_string()
            } else {
                format!("erreur : {}", ToolError::Rejected)
            }),
            (PauseRequest::AskQuestions { .. }, PauseAnswer::Questions(answers)) => {
                serde_json::to_string(
                    &answers
                        .iter()
                        .map(|answer| {
                            serde_json::json!({
                                "question_id": answer.question_id,
                                "value": answer.value,
                                "unsatisfactory_reason": answer.unsatisfactory_reason,
                            })
                        })
                        .collect::<Vec<_>>(),
                )
                .map_err(|error| {
                    AgentError::Model(ModelError::InvalidResponse(format!(
                        "échec de sérialisation des réponses : {error}"
                    )))
                })
            }
            (PauseRequest::RequestDocument { .. }, PauseAnswer::Document(document)) => {
                serde_json::to_string(document).map_err(|error| {
                    AgentError::Model(ModelError::InvalidResponse(format!(
                        "échec de sérialisation du document : {error}"
                    )))
                })
            }
            _ => Err(AgentError::MismatchedAnswer),
        }
    }

    /// Consomme le flux d'un tour de modèle jusqu'à son terme, en notifiant
    /// [`Self::observer`] (labellisé `agent_label`) de chaque fragment de
    /// réflexion/contenu reçu, et accumule les fragments d'appels d'outils en
    /// [`ToolCall`]s complets une fois le flux terminé.
    async fn run_turn(
        &self,
        agent_label: &str,
        messages: &[ChatMessage],
        tool_definitions: &[ToolDefinition],
    ) -> Result<ChatMessage, AgentError> {
        let mut events = self.model.stream(messages, tool_definitions).await?;

        let mut content = String::new();
        let mut tool_calls: BTreeMap<usize, PartialToolCall> = BTreeMap::new();

        while let Some(event) = events.recv().await {
            match event? {
                StreamEvent::ReasoningDelta(delta) => {
                    self.observer.on_reasoning_delta(agent_label, &delta).await;
                }
                StreamEvent::ContentDelta(delta) => {
                    self.observer.on_content_delta(agent_label, &delta).await;
                    content.push_str(&delta);
                }
                StreamEvent::ToolCallDelta {
                    index,
                    id,
                    name,
                    arguments_delta,
                } => {
                    let entry = tool_calls.entry(index).or_default();
                    if let Some(id) = id {
                        entry.id = id;
                    }
                    if let Some(name) = name {
                        entry.name = name;
                    }
                    if let Some(fragment) = arguments_delta {
                        entry.arguments.push_str(&fragment);
                    }
                }
            }
        }

        let tool_calls = tool_calls
            .into_values()
            .map(PartialToolCall::finish)
            .collect::<Result<Vec<_>, ModelError>>()?;

        Ok(ChatMessage {
            role: Role::Assistant,
            content: (!content.is_empty()).then_some(content),
            tool_calls,
            tool_call_id: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::observer::NullAgentObserver;
    use crate::tool::{Tool, ToolOutput};
    use crate::tools::{AskUserTool, DelegateToExpertTool, SpawnExpertTool};
    use async_trait::async_trait;
    use serde_json::{Value, json};
    use tokio::sync::mpsc;

    /// Modèle scripté qui renvoie ses réponses préparées dans l'ordre,
    /// indépendamment du frame qui l'interroge : suffisant pour dérouler un
    /// scénario de délégation dont on connaît à l'avance la séquence exacte
    /// de tours (voir chaque test pour le détail de la séquence attendue).
    struct ScriptedModel {
        responses: std::sync::Mutex<Vec<ChatMessage>>,
    }

    impl ScriptedModel {
        fn new(responses: Vec<ChatMessage>) -> Arc<Self> {
            Arc::new(Self {
                responses: std::sync::Mutex::new(responses),
            })
        }
    }

    #[async_trait]
    impl LanguageModel for ScriptedModel {
        fn model_name(&self) -> &str {
            "scripted"
        }

        async fn stream(
            &self,
            _messages: &[ChatMessage],
            _tools: &[ToolDefinition],
        ) -> Result<mpsc::UnboundedReceiver<Result<StreamEvent, ModelError>>, ModelError> {
            let response = self
                .responses
                .lock()
                .expect("verrou non empoisonné")
                .remove(0);

            let (tx, rx) = mpsc::unbounded_channel();
            if let Some(content) = response.content {
                let _ = tx.send(Ok(StreamEvent::ContentDelta(content)));
            }
            for (index, call) in response.tool_calls.into_iter().enumerate() {
                let _ = tx.send(Ok(StreamEvent::ToolCallDelta {
                    index,
                    id: Some(call.id),
                    name: Some(call.name),
                    arguments_delta: Some(call.arguments.to_string()),
                }));
            }
            Ok(rx)
        }
    }

    fn final_answer(text: &str) -> ChatMessage {
        ChatMessage {
            role: Role::Assistant,
            content: Some(text.to_string()),
            tool_calls: Vec::new(),
            tool_call_id: None,
        }
    }

    fn tool_call(id: &str, name: &str, arguments: Value) -> ChatMessage {
        ChatMessage::assistant_tool_calls(vec![ToolCall {
            id: id.to_string(),
            name: name.to_string(),
            arguments,
        }])
    }

    struct StaticCatalog(Vec<AgentProfile>);

    #[async_trait]
    impl AgentCatalog for StaticCatalog {
        async fn list(&self) -> Result<Vec<AgentProfile>, ToolError> {
            Ok(self.0.clone())
        }

        async fn get(&self, id: &str) -> Result<Option<AgentProfile>, ToolError> {
            Ok(self.0.iter().find(|profile| profile.id == id).cloned())
        }
    }

    fn expert_a_profile(max_steps: u32) -> AgentProfile {
        AgentProfile {
            id: "expert_a".to_string(),
            display_name: "Expert A".to_string(),
            system_prompt: "Tu es l'expert A.".to_string(),
            tool_names: vec![
                "ask_user".to_string(),
                "loop_tool".to_string(),
                "spawn_expert".to_string(),
            ],
            max_steps,
        }
    }

    /// Outil qui boucle indéfiniment sans jamais réclamer d'interaction
    /// humaine ni de délégation, pour exercer `MaxStepsExceeded` sans avoir à
    /// scripter un nombre de réponses modèle égal à la limite testée.
    struct LoopTool;

    #[async_trait]
    impl Tool for LoopTool {
        fn name(&self) -> &str {
            "loop_tool"
        }
        fn description(&self) -> &str {
            "outil de test qui ne fait rien"
        }
        fn parameters_schema(&self) -> Value {
            json!({ "type": "object" })
        }
        async fn call(&self, _arguments: Value) -> Result<ToolOutput, ToolError> {
            Ok(ToolOutput::new("ok"))
        }
    }

    fn orchestrator(model: Arc<dyn LanguageModel>, catalog: StaticCatalog) -> Orchestrator {
        let profiles = catalog.0.clone();
        let mut tools = ToolRegistry::new();
        tools.register(Box::new(DelegateToExpertTool::new(&profiles)));
        tools.register(Box::new(SpawnExpertTool));
        tools.register(Box::new(AskUserTool));
        tools.register(Box::new(LoopTool));
        Orchestrator::new(
            model,
            tools,
            Arc::new(catalog),
            Arc::new(NullAgentObserver),
            Arc::new(AtomicBool::new(false)),
        )
    }

    #[tokio::test]
    async fn delegates_to_an_expert_and_bubbles_its_answer_back_to_the_supervisor() {
        let model = ScriptedModel::new(vec![
            tool_call(
                "call_1",
                "delegate_to_expert",
                json!({ "expert_id": "expert_a", "task": "fais x" }),
            ),
            final_answer("réponse de l'expert"),
            final_answer("terminé"),
        ]);
        let orchestrator = orchestrator(model, StaticCatalog(vec![expert_a_profile(8)]));

        let mut run = OrchestrationRun::new(AgentFrame::supervisor(
            "tu es le superviseur",
            vec!["delegate_to_expert".to_string()],
            8,
            "rédige l'acte",
        ));

        let outcome = orchestrator
            .drive(&mut run)
            .await
            .expect("exécution réussie");
        assert!(matches!(outcome, RunOutcome::Done(text) if text == "terminé"));
        assert_eq!(run.status, RunStatus::Done);
        assert_eq!(
            run.stack.len(),
            1,
            "le frame expert doit avoir été dépilé, mais pas le superviseur (historique conservé)"
        );
    }

    #[tokio::test]
    async fn a_delegated_expert_s_narration_spread_over_several_turns_is_not_truncated() {
        // L'expert répartit sa synthèse sur deux tours narrés en marge d'un
        // appel d'outil, avant de conclure par un dernier tour minimal : la
        // réponse bubblée au Superviseur doit reprendre l'ensemble, pas
        // seulement ce dernier tour (voir `frame_answer_text`).
        let model = ScriptedModel::new(vec![
            tool_call(
                "call_1",
                "delegate_to_expert",
                json!({ "expert_id": "expert_a", "task": "fais x" }),
            ),
            ChatMessage {
                role: Role::Assistant,
                content: Some("première partie de la réponse".to_string()),
                tool_calls: vec![ToolCall {
                    id: "call_2".to_string(),
                    name: "loop_tool".to_string(),
                    arguments: json!({}),
                }],
                tool_call_id: None,
            },
            final_answer("terminé."),
            final_answer("terminé"),
        ]);
        let orchestrator = orchestrator(model, StaticCatalog(vec![expert_a_profile(8)]));

        let mut run = OrchestrationRun::new(AgentFrame::supervisor(
            "tu es le superviseur",
            vec!["delegate_to_expert".to_string()],
            8,
            "rédige l'acte",
        ));

        orchestrator
            .drive(&mut run)
            .await
            .expect("exécution réussie");

        let bubbled = run.stack[0]
            .history
            .iter()
            .find_map(|message| match &message.tool_call_id {
                Some(id) if id == "call_1" => message.content.clone(),
                _ => None,
            })
            .expect("résultat de l'appel delegate_to_expert absent de l'historique");
        assert_eq!(
            bubbled, "première partie de la réponse\n\nterminé.",
            "la réponse bubblée doit reprendre tous les tours narrés de l'expert, pas seulement le dernier"
        );
    }

    #[tokio::test]
    async fn pauses_on_a_nested_expert_question_and_resumes_up_to_the_supervisor() {
        let model = ScriptedModel::new(vec![
            tool_call(
                "call_1",
                "delegate_to_expert",
                json!({ "expert_id": "expert_a", "task": "fais x" }),
            ),
            tool_call(
                "call_2",
                "ask_user",
                json!({ "question": "quelle valeur ?" }),
            ),
            final_answer("42 reçu"),
            final_answer("terminé"),
        ]);
        let orchestrator = orchestrator(model, StaticCatalog(vec![expert_a_profile(8)]));

        let mut run = OrchestrationRun::new(AgentFrame::supervisor(
            "tu es le superviseur",
            vec!["delegate_to_expert".to_string()],
            8,
            "rédige l'acte",
        ));

        let outcome = orchestrator
            .drive(&mut run)
            .await
            .expect("exécution réussie");
        let (agent_label, question) = match outcome {
            RunOutcome::Paused {
                agent_label,
                request: PauseRequest::Ask { question },
            } => (agent_label, question),
            other => panic!("attendu une pause, obtenu {other:?}"),
        };
        assert_eq!(agent_label, "Expert A");
        assert_eq!(question, "quelle valeur ?");
        assert_eq!(run.status, RunStatus::Paused);
        assert_eq!(run.stack.len(), 2, "superviseur + expert en attente");

        let outcome = orchestrator
            .resume(&mut run, PauseAnswer::Text("42".to_string()))
            .await
            .expect("reprise réussie");
        assert!(matches!(outcome, RunOutcome::Done(text) if text == "terminé"));
        assert_eq!(run.status, RunStatus::Done);
        assert_eq!(
            run.stack.len(),
            1,
            "seul le superviseur reste, son historique conservé"
        );
    }

    #[tokio::test]
    async fn max_steps_is_enforced_per_frame() {
        // Le superviseur ne délègue qu'une fois ; c'est l'expert (max_steps
        // = 1) qui boucle indéfiniment sur `loop_tool` sans jamais répondre.
        let model = ScriptedModel::new(vec![
            tool_call(
                "call_1",
                "delegate_to_expert",
                json!({ "expert_id": "expert_a", "task": "fais x" }),
            ),
            tool_call("call_2", "loop_tool", json!({})),
            tool_call("call_3", "loop_tool", json!({})),
        ]);
        let orchestrator = orchestrator(model, StaticCatalog(vec![expert_a_profile(1)]));

        let mut run = OrchestrationRun::new(AgentFrame::supervisor(
            "tu es le superviseur",
            vec!["delegate_to_expert".to_string()],
            8,
            "rédige l'acte",
        ));

        let error = orchestrator
            .drive(&mut run)
            .await
            .expect_err("doit échouer");
        assert!(matches!(error, AgentError::MaxStepsExceeded(1)));
    }

    #[tokio::test]
    async fn a_finished_supervisor_frame_can_be_resumed_as_a_new_task_with_its_history_kept() {
        let model = ScriptedModel::new(vec![
            final_answer("première réponse"),
            final_answer("seconde réponse"),
        ]);
        let orchestrator = orchestrator(model, StaticCatalog(Vec::new()));

        let mut run = OrchestrationRun::new(AgentFrame::supervisor(
            "tu es le superviseur",
            Vec::new(),
            8,
            "premier message",
        ));
        orchestrator
            .drive(&mut run)
            .await
            .expect("exécution réussie");
        assert_eq!(run.status, RunStatus::Done);

        let root = run
            .stack
            .pop()
            .expect("le superviseur terminé reste sur la pile");
        let mut run = OrchestrationRun::new(root.resume_as_new_task("second message"));
        let outcome = orchestrator
            .drive(&mut run)
            .await
            .expect("exécution réussie");

        assert!(matches!(outcome, RunOutcome::Done(text) if text == "seconde réponse"));
        // système + 1er message + 1ère réponse + 2e message + 2e réponse
        assert_eq!(run.stack[0].history.len(), 5);
    }

    #[test]
    fn nested_supervisor_copies_template_config_with_fresh_history_and_steps() {
        let template = AgentFrame::supervisor(
            "tu es le superviseur",
            vec!["delegate_to_expert".to_string()],
            8,
            "tâche initiale",
        );

        let nested = AgentFrame::nested_supervisor(&template, "sous-tâche");

        assert_eq!(nested.agent_label, "Superviseur");
        assert_eq!(nested.profile_id, None);
        assert_eq!(nested.system_prompt, template.system_prompt);
        assert_eq!(nested.tool_names, template.tool_names);
        assert_eq!(nested.max_steps, template.max_steps);
        assert_eq!(nested.steps_taken, 0);
        assert!(nested.pending.is_none());
        // système + tâche : historique propre, indépendant de `template`.
        assert_eq!(nested.history.len(), 2);
    }

    #[tokio::test]
    async fn resolve_delegate_target_allows_delegation_below_max_stack_depth() {
        let orchestrator = orchestrator(ScriptedModel::new(Vec::new()), StaticCatalog(Vec::new()));
        let root = AgentFrame::supervisor("tu es le superviseur", Vec::new(), 8, "tâche");
        let run = OrchestrationRun::new(root);

        let request = DelegateRequest {
            target: DelegateTarget::Supervisor,
            task: "sous-tâche".to_string(),
        };
        let child = orchestrator
            .resolve_delegate_target(&run, &request)
            .await
            .expect("la délégation doit réussir sous la profondeur maximale");

        assert_eq!(child.agent_label, "Superviseur");
        assert_eq!(child.profile_id, None);
    }

    #[tokio::test]
    async fn resolve_delegate_target_rejects_once_max_stack_depth_is_reached() {
        let orchestrator = orchestrator(ScriptedModel::new(Vec::new()), StaticCatalog(Vec::new()));
        let root = AgentFrame::supervisor("tu es le superviseur", Vec::new(), 8, "tâche");
        let mut run = OrchestrationRun::new(root.clone());
        for _ in 1..MAX_STACK_DEPTH {
            run.stack.push(root.clone());
        }
        assert_eq!(run.stack.len(), MAX_STACK_DEPTH);

        let request = DelegateRequest {
            target: DelegateTarget::Supervisor,
            task: "encore".to_string(),
        };
        let error = orchestrator
            .resolve_delegate_target(&run, &request)
            .await
            .expect_err("la profondeur maximale doit être refusée");
        assert!(error.contains("profondeur maximale"));
    }

    #[tokio::test]
    async fn spawn_expert_creates_a_nested_supervisor_whose_answer_bubbles_back_to_the_caller() {
        // Superviseur -> délègue à Expert A -> Expert A confie une sous-tâche
        // dynamique via `spawn_expert` -> le Superviseur imbriqué répond
        // directement (sans redéléguer) -> sa réponse remonte à Expert A ->
        // la réponse d'Expert A remonte au Superviseur racine.
        let model = ScriptedModel::new(vec![
            tool_call(
                "call_1",
                "delegate_to_expert",
                json!({ "expert_id": "expert_a", "task": "fais x" }),
            ),
            tool_call("call_2", "spawn_expert", json!({ "task": "fais y" })),
            final_answer("réponse du superviseur imbriqué"),
            final_answer("réponse de l'expert A"),
            final_answer("terminé"),
        ]);
        let orchestrator = orchestrator(model, StaticCatalog(vec![expert_a_profile(8)]));

        let mut run = OrchestrationRun::new(AgentFrame::supervisor(
            "tu es le superviseur",
            vec!["delegate_to_expert".to_string()],
            8,
            "rédige l'acte",
        ));

        let outcome = orchestrator
            .drive(&mut run)
            .await
            .expect("exécution réussie");
        assert!(matches!(outcome, RunOutcome::Done(text) if text == "terminé"));
        assert_eq!(run.status, RunStatus::Done);
        assert_eq!(
            run.stack.len(),
            1,
            "les frames expert et superviseur imbriqué doivent tous deux avoir été dépilés"
        );
    }
}
