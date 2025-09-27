use std::rc::Rc;

use agent_client_protocol::AgentSideConnection;
use tokio::{io, task::LocalSet};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

mod amp_agent;
use amp_agent::AmpAgent;

#[tokio::main]
async fn main() -> io::Result<()> {
    let stdin = tokio::io::stdin().compat();
    let stdout = tokio::io::stdout().compat_write();

    let amp_agent = Rc::new(AmpAgent::new());

    LocalSet::new()
        .run_until(async move {
            let (client, io_task) =
                AgentSideConnection::new(amp_agent.clone(), stdout, stdin, |fut| {
                    tokio::task::spawn_local(fut);
                });

            amp_agent.set_client(Rc::new(client));
            io_task
                .await
                .map_err(|e| std::io::Error::other(format!("ACP I/O error: {e}")))
        })
        .await?;

    Ok(())
}
