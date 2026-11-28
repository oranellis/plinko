// modules {{{
use std::collections::HashMap;

use uuid::Uuid;

use crate::{
    common::short_id::ShortId,
    planner::items::dependency::PlanNode,
    scheduler::state::UnscheduledReason,
    scheduler::{
        arc::{Arc, ArcId},
        day::Day,
    },
};
// }}}

/// Every plan of a tenant as one network, indexed densely so a run is array lookups
#[derive(Clone, Debug, Default)]
pub struct Network {
    pub vertices: Vec<Vertex>,
    pub arcs: Vec<Arc>,
    /// The start of today, which what starts a plan waits on
    pub root: VertexId,
    /// In id order
    pub plans: Vec<PlanInfo>,
    /// Keyed by the index of the plan in `plans`
    pub lowered: HashMap<(u32, PlanNode), Lowered>,
    /// Left out before scheduling, like dropped tasks or tasks no one can work
    pub unscheduled: HashMap<(u32, PlanNode), UnscheduledReason>,
    /// Topological: every vertex after everything it waits on
    pub order: Vec<VertexId>,
}

// Network supporting types {{{
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct VertexId(pub u32);

/// A day boundary: "these things are done". Nodes with the same dependencies share one, so only
/// a milestone's or group's own `earliest` ever needs a vertex of its own.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Vertex {
    /// The start of its group, waited for like the rest. It does not make the vertex scheduled:
    /// with other inputs and none of them scheduled, it is not either.
    pub anchor: Option<VertexId>,
    pub after: Vec<VertexId>,
    pub incoming: Vec<ArcId>,
    /// The vertices that wait on this one
    pub next: Vec<VertexId>,
    pub outgoing: Vec<ArcId>,
    pub earliest: Option<Day>,
    /// Limits what comes before it without moving anything
    pub deadline: Option<Day>,
    /// Where it is whatever it waits on
    pub pinned: Option<Day>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlanInfo {
    pub id: ShortId,
    /// Waits on everything in the plan
    pub end: VertexId,
    /// Days added to how late the plan's goals count as
    pub priority: i64,
    pub targets: Vec<Target>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Target {
    pub id: Uuid,
    pub vertex: VertexId,
    pub priority: i64,
}

/// A milestone has the same start and end, a group spans its members
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lowered {
    pub start: VertexId,
    pub end: VertexId,
    pub arc: Option<ArcId>,
}
// }}}

// impl Vertex {{{
impl Vertex {
    /// Its group's start included
    pub fn before(&self) -> impl Iterator<Item = VertexId> + '_ {
        self.after.iter().copied().chain(self.anchor)
    }

    pub fn inputs(&self) -> usize {
        self.before().count() + self.incoming.len()
    }
}
// }}}

// impl Network {{{
impl Network {
    pub fn add_vertex(&mut self, vertex: Vertex) -> VertexId {
        self.vertices.push(vertex);
        VertexId(self.vertices.len() as u32 - 1)
    }

    pub fn add_arc(&mut self, arc: Arc) -> ArcId {
        self.arcs.push(arc);
        ArcId(self.arcs.len() as u32 - 1)
    }

    pub fn vertex(&self, id: VertexId) -> &Vertex {
        &self.vertices[id.0 as usize]
    }

    pub fn vertex_mut(&mut self, id: VertexId) -> &mut Vertex {
        &mut self.vertices[id.0 as usize]
    }

    pub fn arc(&self, id: ArcId) -> &Arc {
        &self.arcs[id.0 as usize]
    }

    pub fn arc_ids(&self) -> impl Iterator<Item = ArcId> + use<> {
        (0..self.arcs.len() as u32).map(ArcId)
    }

    /// Fills in the back links and `order` once everything is added. Vertices on a cycle are
    /// left out of the order.
    pub fn link(&mut self) {
        for v in &mut self.vertices {
            v.next.clear();
            v.outgoing.clear();
            v.incoming.clear();
        }
        for i in 0..self.vertices.len() {
            let before: Vec<VertexId> = self.vertices[i].before().collect();
            for from in before {
                self.vertex_mut(from).next.push(VertexId(i as u32));
            }
        }
        for i in 0..self.arcs.len() {
            let (start, end) = (self.arcs[i].start, self.arcs[i].end);
            self.vertex_mut(start).outgoing.push(ArcId(i as u32));
            self.vertex_mut(end).incoming.push(ArcId(i as u32));
        }
        let mut waiting: Vec<usize> = self.vertices.iter().map(Vertex::inputs).collect();
        let mut ready: Vec<VertexId> = (0..self.vertices.len())
            .rev()
            .filter(|i| waiting[*i] == 0)
            .map(|i| VertexId(i as u32))
            .collect();
        self.order = Vec::with_capacity(self.vertices.len());
        while let Some(v) = ready.pop() {
            self.order.push(v);
            let vertex = &self.vertices[v.0 as usize];
            let ends = vertex.outgoing.iter().map(|a| self.arcs[a.0 as usize].end);
            for next in vertex.next.iter().copied().chain(ends) {
                waiting[next.0 as usize] -= 1;
                if waiting[next.0 as usize] == 0 {
                    ready.push(next);
                }
            }
        }
    }
}
// }}}

// tests {{{
#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::scheduler::{
        arc::{ArcKind, Work},
        table::RowId,
    };

    pub fn wait(net: &mut Network, start: VertexId, days: u32) -> VertexId {
        let end = net.add_vertex(Vertex::default());
        net.add_arc(Arc {
            node: PlanNode::PlanDelay(Uuid::from_u128(u128::from(end.0))),
            plan: 0,
            start,
            end,
            kind: ArcKind::Work(Work::wait(days, RowId(0))),
        });
        end
    }

    #[test]
    fn the_order_visits_a_vertex_after_all_it_waits_on() {
        let mut net = Network::default();
        let root = net.add_vertex(Vertex::default());
        let a = wait(&mut net, root, 3);
        let b = wait(&mut net, root, 5);
        let join = net.add_vertex(Vertex {
            after: vec![a, b],
            ..Vertex::default()
        });
        let c = wait(&mut net, join, 2);
        net.link();
        let at = |v: VertexId| net.order.iter().position(|o| *o == v).unwrap();
        assert_eq!(net.order.len(), 5);
        assert!(at(root) < at(a) && at(root) < at(b));
        assert!(at(a) < at(join) && at(b) < at(join) && at(join) < at(c));
        assert_eq!(net.vertex(a).next, [join]);
        assert_eq!(net.vertex(join).outgoing.len(), 1);
    }
}
// }}}
