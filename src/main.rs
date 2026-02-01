use clap::Parser;
use std::process::ExitCode;

use janus::cli::{
    CacheAction, ConfigAction, DepAction, HookAction, LinkAction, PlanAction, RemoteAction,
    generate_completions,
};
use janus::cli::{Cli, Commands};
#[cfg(feature = "semantic-search")]
use janus::commands::cmd_search;
use janus::commands::{
    cmd_add_note, cmd_adopt, cmd_board, cmd_cache_clear, cmd_cache_path, cmd_cache_rebuild,
    cmd_cache_status, cmd_close, cmd_config_get, cmd_config_set, cmd_config_show, cmd_create,
    cmd_dep_add, cmd_dep_remove, cmd_dep_tree, cmd_doctor, cmd_edit, cmd_graph, cmd_hook_disable,
    cmd_hook_enable, cmd_hook_install, cmd_hook_list, cmd_hook_log, cmd_hook_run, cmd_link_add,
    cmd_link_remove, cmd_ls, cmd_next, cmd_plan_add_phase, cmd_plan_add_ticket, cmd_plan_create,
    cmd_plan_delete, cmd_plan_edit, cmd_plan_import, cmd_plan_ls, cmd_plan_move_ticket,
    cmd_plan_next, cmd_plan_remove_phase, cmd_plan_remove_ticket, cmd_plan_rename,
    cmd_plan_reorder, cmd_plan_show, cmd_plan_status, cmd_plan_verify, cmd_push, cmd_query,
    cmd_remote_browse, cmd_remote_link, cmd_reopen, cmd_set, cmd_show, cmd_show_import_spec,
    cmd_start, cmd_status, cmd_sync, cmd_view,
};

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Create {
            title,
            description,
            design,
            acceptance,
            priority,
            ticket_type,
            external_ref,
            parent,
            prefix,
            spawned_from,
            spawn_context,
            size,
            json,
        } => cmd_create(
            title,
            description,
            design,
            acceptance,
            priority,
            ticket_type,
            external_ref,
            parent,
            prefix,
            spawned_from,
            spawn_context,
            size,
            json,
        ),

        Commands::Show { id, json } => cmd_show(&id, json).await,
        Commands::Edit { id, json } => cmd_edit(&id, json).await,
        Commands::AddNote { id, text, json } => {
            let note_text = if text.is_empty() {
                None
            } else {
                Some(text.join(" "))
            };
            cmd_add_note(&id, note_text.as_deref(), json).await
        }

        Commands::Start { id, json } => cmd_start(&id, json).await,
        Commands::Close {
            id,
            summary,
            no_summary,
            cancel,
            json,
        } => cmd_close(&id, summary.as_deref(), no_summary, cancel, json).await,
        Commands::Reopen { id, json } => cmd_reopen(&id, json).await,
        Commands::Status { id, status, json } => cmd_status(&id, &status, json).await,
        Commands::Set {
            id,
            field,
            value,
            json,
        } => cmd_set(&id, &field, value.as_deref(), json).await,

        Commands::Dep { action } => match action {
            DepAction::Add { id, dep_id, json } => cmd_dep_add(&id, &dep_id, json).await,
            DepAction::Remove { id, dep_id, json } => cmd_dep_remove(&id, &dep_id, json).await,
            DepAction::Tree { id, full, json } => cmd_dep_tree(&id, full, json).await,
        },

        Commands::Link { action } => match action {
            LinkAction::Add { ids, json } => cmd_link_add(&ids, json).await,
            LinkAction::Remove { id1, id2, json } => cmd_link_remove(&id1, &id2, json).await,
        },

        Commands::Ls {
            ready,
            blocked,
            closed,
            active,
            status,
            spawned_from,
            depth,
            max_depth,
            next_in_plan,
            phase,
            triaged,
            size,
            limit,
            sort_by,
            json,
        } => {
            cmd_ls(
                ready,
                blocked,
                closed,
                active,
                status.as_deref(),
                spawned_from.as_deref(),
                depth,
                max_depth,
                next_in_plan.as_deref(),
                phase,
                triaged.as_deref(),
                size,
                limit,
                sort_by.as_str(),
                json,
            )
            .await
        }

        Commands::Query { filter } => cmd_query(filter.as_deref()).await,

        Commands::View => cmd_view().await,
        Commands::Board => cmd_board().await,

        Commands::Remote { action } => match action {
            RemoteAction::Browse { provider } => cmd_remote_browse(provider.as_deref()).await,
            RemoteAction::Adopt {
                remote_ref,
                prefix,
                json,
            } => cmd_adopt(&remote_ref, prefix.as_deref(), json).await,
            RemoteAction::Push { id, json } => cmd_push(&id, json).await,
            RemoteAction::Link {
                id,
                remote_ref,
                json,
            } => cmd_remote_link(&id, &remote_ref, json).await,
            RemoteAction::Sync { id, json } => cmd_sync(&id, json).await,
        },

        Commands::Config { action } => match action {
            ConfigAction::Show { json } => cmd_config_show(json),
            ConfigAction::Set { key, value, json } => cmd_config_set(&key, &value, json),
            ConfigAction::Get { key, json } => cmd_config_get(&key, json),
        },

        Commands::Cache { action } => match action {
            CacheAction::Status { json } => cmd_cache_status(json).await,
            CacheAction::Clear { json } => cmd_cache_clear(json).await,
            CacheAction::Rebuild { json } => cmd_cache_rebuild(json).await,
            CacheAction::Path { json } => cmd_cache_path(json).await,
        },

        Commands::Hook { action } => match action {
            HookAction::List { json } => cmd_hook_list(json),
            HookAction::Install {
                recipe,
                force,
                json,
            } => cmd_hook_install(&recipe, force, json).await,
            HookAction::Run { event, id } => cmd_hook_run(&event, id.as_deref()).await,
            HookAction::Enable { json } => cmd_hook_enable(json),
            HookAction::Disable { json } => cmd_hook_disable(json),
            HookAction::Log { lines, json } => cmd_hook_log(lines, json),
        },

        Commands::Doctor { json } => {
            match cmd_doctor(json) {
                Ok((valid, _)) => {
                    if valid {
                        Ok(())
                    } else {
                        // Return error for verification failures
                        Err(janus::error::JanusError::Other(
                            "Ticket health check failed - some files have errors".to_string(),
                        ))
                    }
                }
                Err(e) => Err(e),
            }
        }

        Commands::Plan { action } => match action {
            PlanAction::Create {
                title,
                phases,
                json,
            } => cmd_plan_create(&title, &phases, json),
            PlanAction::Show {
                id,
                raw,
                tickets_only,
                phases_only,
                verbose_phases,
                json,
            } => cmd_plan_show(&id, raw, tickets_only, phases_only, &verbose_phases, json).await,
            PlanAction::Edit { id, json } => cmd_plan_edit(&id, json).await,
            PlanAction::Ls { status, json } => cmd_plan_ls(status.as_deref(), json).await,
            PlanAction::AddTicket {
                plan_id,
                ticket_id,
                phase,
                after,
                position,
                json,
            } => {
                cmd_plan_add_ticket(
                    &plan_id,
                    &ticket_id,
                    phase.as_deref(),
                    after.as_deref(),
                    position,
                    json,
                )
                .await
            }
            PlanAction::RemoveTicket {
                plan_id,
                ticket_id,
                json,
            } => cmd_plan_remove_ticket(&plan_id, &ticket_id, json).await,
            PlanAction::MoveTicket {
                plan_id,
                ticket_id,
                to_phase,
                after,
                position,
                json,
            } => {
                cmd_plan_move_ticket(
                    &plan_id,
                    &ticket_id,
                    &to_phase,
                    after.as_deref(),
                    position,
                    json,
                )
                .await
            }
            PlanAction::AddPhase {
                plan_id,
                phase_name,
                after,
                position,
                json,
            } => cmd_plan_add_phase(&plan_id, &phase_name, after.as_deref(), position, json).await,
            PlanAction::RemovePhase {
                plan_id,
                phase,
                force,
                migrate,
                json,
            } => cmd_plan_remove_phase(&plan_id, &phase, force, migrate.as_deref(), json).await,
            PlanAction::Reorder {
                plan_id,
                phase,
                reorder_phases,
                json,
            } => cmd_plan_reorder(&plan_id, phase.as_deref(), reorder_phases, json).await,
            PlanAction::Delete { id, force, json } => cmd_plan_delete(&id, force, json).await,
            PlanAction::Rename {
                id,
                new_title,
                json,
            } => cmd_plan_rename(&id, &new_title, json).await,
            PlanAction::Next {
                id,
                phase,
                all,
                count,
                json,
            } => cmd_plan_next(&id, phase, all, count, json).await,
            PlanAction::Status { id, json } => cmd_plan_status(&id, json).await,
            PlanAction::Import {
                file,
                dry_run,
                title,
                ticket_type,
                prefix,
                json,
            } => {
                cmd_plan_import(
                    &file,
                    dry_run,
                    title.as_deref(),
                    ticket_type,
                    prefix.as_deref(),
                    json,
                )
                .await
            }
            PlanAction::ImportSpec => cmd_show_import_spec(),
            PlanAction::Verify { json } => {
                match cmd_plan_verify(json) {
                    Ok((valid, _)) => {
                        if valid {
                            Ok(())
                        } else {
                            // Return error for verification failures
                            Err(janus::error::JanusError::Other(
                                "Plan verification failed - some files have errors".to_string(),
                            ))
                        }
                    }
                    Err(e) => Err(e),
                }
            }
        },

        Commands::Graph {
            deps,
            spawn,
            all,
            format,
            root,
            plan,
            json,
        } => {
            cmd_graph(
                deps,
                spawn,
                all,
                &format,
                root.as_deref(),
                plan.as_deref(),
                json,
            )
            .await
        }

        Commands::Next { limit, json } => cmd_next(limit, json).await,

        Commands::Completions { shell } => {
            generate_completions(shell);
            Ok(())
        }

        Commands::Mcp { version } => {
            if version {
                janus::mcp::cmd_mcp_version()
            } else {
                janus::mcp::cmd_mcp().await
            }
        }

        #[cfg(feature = "semantic-search")]
        Commands::Search {
            query,
            limit,
            threshold,
            json,
        } => cmd_search(&query, limit, threshold, json).await,
    };

    match result {
        Ok(_) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{}", e);
            ExitCode::FAILURE
        }
    }
}
