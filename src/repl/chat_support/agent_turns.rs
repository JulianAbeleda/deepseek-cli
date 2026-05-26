use super::super::*;

pub(in crate::repl::chat) fn spawn_docked_turn(
    prior_messages: &[Message],
    prompt: String,
    selected_root: Option<&Path>,
    model: String,
    temperature: Option<f32>,
    legacy_routing: bool,
) -> InFlightTurn {
    if let Some(task) = commands::parse_agent_task_command(&prompt) {
        if let Some(root) = task_root_for_prompt_with_source(task, selected_root) {
            if path_boundary_violation(task, &root.path).is_none() {
                return spawn_agent_turn_with_root_note(
                    task.to_string(),
                    root.path,
                    root.source.fuzzy_note(),
                    model,
                    temperature,
                );
            }
        }
    }
    if !legacy_routing {
        if let Some(root) = model_decided_root_for_prompt_with_source(&prompt, selected_root) {
            if path_boundary_violation(&prompt, &root.path).is_none() {
                return spawn_agent_turn_with_root_note(
                    prompt,
                    root.path,
                    root.source.fuzzy_note(),
                    model,
                    temperature,
                );
            }
        }
    }
    spawn_prompt_turn(prior_messages, prompt, model, temperature)
}

pub(in crate::repl::chat) fn spawn_agent_turn(
    prompt: String,
    root: PathBuf,
    model: String,
    temperature: Option<f32>,
) -> InFlightTurn {
    spawn_agent_turn_with_root_note(prompt, root, None, model, temperature)
}

pub(in crate::repl::chat) fn spawn_agent_turn_with_root_note(
    prompt: String,
    root: PathBuf,
    root_note: Option<String>,
    model: String,
    temperature: Option<f32>,
) -> InFlightTurn {
    let (sender, receiver) = mpsc::channel();
    let cancel = CancellationToken::new();
    let worker_cancel = cancel.clone();
    thread::spawn(move || {
        let result = run_agent_streaming_with_root_note(
            &prompt,
            root,
            root_note,
            &model,
            temperature,
            sender.clone(),
            worker_cancel,
        );
        let _ = sender.send(TurnEvent::Complete(result));
    });
    InFlightTurn { receiver, cancel }
}
