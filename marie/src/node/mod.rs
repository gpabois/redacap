use libp2p::{mdns, request_response, swarm::{NetworkBehaviour, SwarmEvent::*}};
use tokio::select;

use crate::model;

pub async fn run() {

    #[derive(NetworkBehaviour)]
    struct Behaviour {
        mdns: mdns::tokio::Behaviour,
    }

    let mut swarm = libp2p::SwarmBuilder::with_new_identity();
        .with_tokio()
        .with_tcp(
            tcp::Config::default(),
            noise::Config::new,
            yamux::Config::default,
        )?
        .with_behaviour(|key| {
            Ok(Behaviour {
                mdns: mdns::tokio::Behaviour::new(
                    mdns::Config::default(),
                    key.public().to_peer_id(),
                )?,
            })
        })?
        .build();

    swarm.listen_on("/ip4/0.0.0.0/tcp/0".parse()?)?;

    loop {
        select! { 

            event = swarm.select_next_some() => match event {
                Behaviour(BehaviourEvent::Mdns(mdns::Event::Discovered(list))) => {}
                
            }
        }
        
    }
}