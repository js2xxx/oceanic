use std::collections::{hash_map::Entry, HashMap};

use petgraph::{
    algo::toposort,
    prelude::{DiGraph, NodeIndex},
};
use quote::ToTokens;
use syn::{parse_str, FnArg, Ident};

use crate::{
    parse::{ProtoItem, ProtoType::*},
    types::Protocol,
};

type NodeMap = HashMap<Ident, usize>;

fn make_graph(items: &[ProtoItem]) -> Result<(DiGraph<usize, ()>, NodeMap), String> {
    let mut map = HashMap::new();
    let mut graph = DiGraph::new();
    let node_indices = items.iter().enumerate().map(|(index, item)| {
        if matches!(item.ty, Protocol(..)) {
            graph.add_node(index)
        } else {
            NodeIndex::end()
        }
    });
    let node_indices = node_indices.collect::<Vec<_>>();
    for (item, &index) in items.iter().zip(&node_indices) {
        match &item.ty {
            Protocol(proto) => {
                for from in &proto.from {
                    let from_ident = &from.segments.last().unwrap().ident;
                    let from = node_indices[match map.entry(from_ident.clone()) {
                        Entry::Occupied(ent) => *ent.get(),
                        Entry::Vacant(ent) => {
                            let pos = items.iter().position(|item| {
                                matches!(
                                    item.ty,
                                    Protocol(ref proto) if proto.ident == *ent.key()
                                )
                            });
                            let pos = pos.ok_or_else(|| {
                                format!(
                                    "Failed to find `{from:?}` in `{:?}`'s dependent protocols",
                                    proto.ident
                                )
                            })?;
                            ent.insert(pos);
                            pos
                        }
                    }];
                    graph.add_edge(from, index, ());
                }
            }
            Item(_) => {}
        }
    }
    Ok((graph, map))
}

fn dependencies(items: &mut [ProtoItem]) -> Result<(), String> {
    #[inline]
    fn proto(items: &mut [ProtoItem], index: usize) -> &mut Protocol {
        match &mut items[index].ty {
            Protocol(proto) => proto,
            _ => unreachable!(),
        }
    }

    let (graph, map) = make_graph(items)?;
    let indices = toposort(&graph, None).map_err(|cycle| {
        format!(
            "Dependency cycle detected, starting from {:?}",
            items[*graph.node_weight(cycle.node_id()).unwrap()]
        )
    })?;
    for index in indices
        .into_iter()
        .map(|index| *graph.node_weight(index).unwrap())
    {
        let froms = proto(items, index).from.clone();
        for from in froms {
            let from_ident = &from.segments.last().unwrap().ident;
            let methods = proto(items, map[from_ident]).method.clone();
            let events = proto(items, map[from_ident]).event.clone();
            proto(items, index).method.extend(methods);
            proto(items, index).event.extend(events);
        }
        let vec = &mut proto(items, index).method;
        vec.sort_by(|a, b| a.ident.cmp(&b.ident));
        vec.dedup_by(|a, b| a.ident == b.ident);

        let vec = &mut proto(items, index).event;
        vec.sort_by_key(|x| x.1);
        vec.dedup_by_key(|x| x.1);
    }

    Ok(())
}

pub fn resolve(items: &mut [ProtoItem]) -> Result<(), String> {
    for item in items.iter_mut() {
        let (proto, methods, events) = match &mut item.ty {
            Protocol(proto) => (&proto.ident, &mut proto.method, &mut proto.event),
            _ => continue,
        };
        let mut prefix = item.parent.as_os_str().to_string_lossy().to_string();
        prefix += ":";
        prefix += &proto.to_string();
        for method in methods {
            let hash = sha256::digest(prefix.clone() + "::" + &method.ident.to_string());
            method.id = u64::from_ne_bytes(hash.as_bytes()[..8].try_into().unwrap());
        }
        for event in events {
            let hash = sha256::digest(event.0.to_token_stream().to_string());
            event.1 = u64::from_ne_bytes(hash.as_bytes()[..8].try_into().unwrap());
        }
    }
    dependencies(items)?;

    for item in items.iter_mut() {
        let (proto, methods) = match &mut item.ty {
            Protocol(proto) => (&proto.ident, &mut proto.method),
            _ => continue,
        };
        for method in methods {
            let client = Ident::new(&(proto.to_string() + "Client"), proto.span()).to_string();
            let server = Ident::new(&(proto.to_string() + "Server"), proto.span()).to_string();
            for arg in &mut method.args {
                let arg = match arg {
                    FnArg::Typed(arg) => arg,
                    _ => return Err("Method arguments cannot be receivers (auto included)".into()),
                };
                let ty = arg.ty.to_token_stream().to_string();
                let ty = ty.replace("SelfClient", &client);
                let ty = ty.replace("SelfServer", &server);
                arg.ty = parse_str(&ty).map_err(|err| err.to_string())?;
            }
            let ty = method.output.to_token_stream().to_string();
            let ty = ty.replace("SelfClient", &client);
            let ty = ty.replace("SelfServer", &server);
            method.output = parse_str(&ty).map_err(|err| err.to_string())?;
        }
    }
    Ok(())
}
