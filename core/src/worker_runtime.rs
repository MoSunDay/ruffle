//! Same-arena Worker emulation (route A, slice 2).
//!
//! Worker SWFs are loaded into a child `ApplicationDomain` and driven from the
//! main thread. This is enough for content whose worker main class uses
//! `Event.ENTER_FRAME` polling plus `Worker.current` shared properties, which
//! is exactly the MU worker protocol.

use crate::avm2::Domain;
use crate::avm2::object::{EventObject, WorkerObject, WorkerState};
use crate::avm2::{Activation as Avm2Activation, Avm2};
use crate::context::UpdateContext;
use crate::display_object::{DisplayObject, MovieClip, Stage, TDisplayObject};
use crate::limits::ExecutionLimit;
use crate::tag_utils::{SwfMovie, SwfMovieData};
use crate::vminterface::Instantiator;
use ruffle_common::tag_utils::LoadBytesInfo;

/// Runs one worker tick: dispatch queued `workerState` events, boot newly
/// started workers, then advance every running worker's timeline.
pub fn tick_workers<'gc>(context: &mut UpdateContext<'gc>) {
    dispatch_worker_state_events(context);
    boot_workers(context);
    run_worker_frames(context);
}

fn dispatch_worker_state_events<'gc>(context: &mut UpdateContext<'gc>) {
    let queue = std::mem::take(&mut context.avm2.worker_state_queue);
    for worker in queue {
        if worker.state() != WorkerState::Running {
            continue;
        }
        let event = EventObject::bare_default_event(context, "workerState");
        Avm2::dispatch_event(context, event, worker.into());
    }
}

fn boot_workers<'gc>(context: &mut UpdateContext<'gc>) {
    let boots = std::mem::take(&mut context.avm2.worker_boot_queue);
    for worker in boots {
        if worker.state() != WorkerState::Running || worker.root().is_some() {
            continue;
        }
        let previous_worker = context.avm2.current_worker;
        context.avm2.current_worker = Some(worker);
        let result = boot_worker(context, worker);
        context.avm2.current_worker = previous_worker;

        match result {
            Ok(root) => {
                worker.set_root(context.gc(), Some(root));
                context.avm2.workers.push(worker);
                context.avm2.worker_state_queue.push(worker);
            }
            Err(e) => {
                tracing::error!("Failed to boot same-arena worker: {e}");
                worker.set_state(context.gc(), WorkerState::Terminated);
            }
        }
    }
}

fn boot_worker<'gc>(
    context: &mut UpdateContext<'gc>,
    worker: WorkerObject<'gc>,
) -> Result<DisplayObject<'gc>, String> {
    let bytes = worker.swf_bytes();
    if bytes.is_empty() {
        return Err("worker SWF bytes are empty".into());
    }

    // `createWorker` receives bytes, so model this as `Loader.loadBytes`: the
    // worker inherits the creator's sandbox, and the absolute URL gives
    // relative lookups a usable base.
    let worker_url = format!("{}#worker", context.root_swf.url());
    let movie_data = SwfMovieData::from_data(
        &bytes,
        worker_url,
        None,
        Some(LoadBytesInfo {
            loader_sandbox_type: context.root_swf.sandbox_type(),
        }),
    )
    .map_err(|e| format!("invalid worker SWF: {e}"))?;
    let movie = SwfMovie::new(context.gc(), movie_data);

    let parent = context.avm2.stage_domain();
    let domain = Domain::movie_domain(context, parent);
    context
        .library
        .library_for_movie_mut(movie)
        .set_avm2_domain(domain);

    let root: DisplayObject = {
        let mut activation = Avm2Activation::from_nothing(context);
        MovieClip::player_root_movie(&mut activation, movie).into()
    };

    // Worker movies arrive as in-memory bytes, so they have never been through
    // the normal Loader preload loop. Preload them here to populate the
    // library, run the `SymbolClass`/`DoABC` tags, and load the first frame;
    // otherwise `construct_frame` never reaches the movie's main class.
    let Some(root_mc) = root.as_movie_clip() else {
        return Err("worker root is not a MovieClip".into());
    };

    // Worker movies are not on the main display list, but still need a Stage
    // ancestor for `stage.frameRate` and friends. This shadow Stage keeps its
    // own frame rate, so it cannot change the main movie's timing.
    let stage = Stage::empty(context.gc(), false, movie);
    stage.post_instantiation(context, None, Instantiator::Movie, false);
    root.set_parent(context, Some(stage.into()));

    let mut limit = ExecutionLimit::none();
    if !root_mc.preload(context, &mut limit) {
        return Err("worker SWF preload did not finish".into());
    }

    Ok(root)
}

fn run_worker_frames<'gc>(context: &mut UpdateContext<'gc>) {
    let workers = context.avm2.workers.clone();
    for worker in workers {
        if worker.state() != WorkerState::Running {
            continue;
        }
        let Some(root) = worker.root() else {
            continue;
        };
        let previous = context.avm2.current_worker;
        context.avm2.current_worker = Some(worker);
        root.enter_frame(context);
        root.construct_frame(context);
        context.avm2.current_worker = previous;
    }
}
