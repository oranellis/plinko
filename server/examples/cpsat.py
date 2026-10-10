"""Solves a tenant exported by the `rich` example with CP-SAT and compares it with the scheduler.
usage: python cpsat.py rich_<plans>_<seed>.json seconds [workers]
writes solution_<plans>_<seed>.json next to it, which `VALIDATE=<dir> cargo run --release --example rich`
checks the scheduler accepts. Needs `pip install ortools`. Settings from the environment:
K people considered for work done by a tag or group (default 3, more costs memory), HX days of room
past the scheduler's plan end, LNS=1 to only improve the scheduler's schedule by neighbourhood
search (much less memory, the way to go past a thousand tasks), MEM megabytes before it stops, LOG=1
for the solver's own log."""
import json, sys, datetime as dt, os
from ortools.sat.python import cp_model

path, seconds = sys.argv[1], float(sys.argv[2])
workers = int(sys.argv[3]) if len(sys.argv) > 3 else 8
data = json.load(open(path))
base = dt.date.fromisoformat(data["start"])
caps = data["caps"]
people = len(caps)
tasks = {t["id"]: t for t in data["tasks"]}
miles = {m["id"]: m["deps"] for m in data["milestones"]}

def date_of(k):  # working day index to calendar date, Monday first
    return base + dt.timedelta(days=(k // 5) * 7 + k % 5)
def cal_days(k):
    return (k // 5) * 7 + k % 5
def index_of(day):
    n = (day - base).days
    return (n // 7) * 5 + n % 7

def flat(ids):
    out, stack = set(), list(ids)
    while stack:
        x = stack.pop()
        if x in tasks:
            out.add(x)
        else:
            stack.extend(miles[x])
    return out

deps = {i: flat(t["deps"]) for i, t in tasks.items()}
ends = flat(data["ends"])
hs = {i: index_of(dt.date.fromisoformat(s)) for i, s in data["heuristic"]["starts"].items()}
hp = {i: p for i, p in data["heuristic"]["people"].items()}
hf = {i: index_of(dt.date.fromisoformat(f) - dt.timedelta(days=1)) + 1 for i, f in data["heuristic"]["finishes"].items()}

def lateness(starts):
    miss, late = 0, 0
    for i, t in tasks.items():
        if t["constraint"]:
            _, day = t["constraint"]
            l = cal_days(starts[i]) - (dt.date.fromisoformat(day) - base).days
            if l > 0:
                miss += 1
                late += l
    return miss, late, max(starts[i] + tasks[i]["days"] for i in ends)

h_miss, h_late, h_end = lateness(hs)
H = h_end + int(os.environ.get("HX", "60"))

m = cp_model.CpModel()
s = {i: m.NewIntVar(1, H, f"s{i}") for i in tasks}
e = {i: m.NewIntVar(2, H + 8, f"e{i}") for i in tasks}
loads = [[] for _ in range(people)]  # (interval, demand) per person
K = int(os.environ.get('K', '3'))
booked = {p: 0 for p in range(people)}
for i, p in hp.items():
    booked[int(p)] += tasks[i]['days'] * tasks[i]['hours']
choice = {}  # task -> [(person, literal)]
for i, t in tasks.items():
    d, h = t["days"], t["hours"]
    cands = []
    for p in t["candidates"]:
        # the weekdays this person can work this task on, which repeat every week
        off = [w for w in range(5) if caps[p][w] >= h and t["allowed"][w]]
        if off:
            cands.append((p, off))
    assert cands, f"nobody can do {i}"
    if len(cands) > K:
        cands = sorted(cands, key=lambda c: (str(c[0]) != str(hp.get(i)), booked[c[0]]))[:K]
    lits = []
    single = len(cands) == 1
    for p, off in cands:
        x = None if single else m.NewBoolVar(f"x{i}_{p}")
        lits.append((p, x))
        if len(off) == 5:
            if single:
                m.Add(e[i] == s[i] + d)
            else:
                m.Add(e[i] == s[i] + d).OnlyEnforceIf(x)
            if single:
                loads[p].append((m.NewIntervalVar(s[i], d, e[i], f"iv{i}"), h))
            else:
                loads[p].append((m.NewOptionalIntervalVar(s[i], d, e[i], x, f"iv{i}_{p}"), h))
        else:
            n = len(off)
            r = m.NewIntVar(0, n * (H // 5 + 1) - d, f"r{i}_{p}")
            ds = []
            chosen = single or str(hp.get(i)) == str(p)
            if chosen and i in hs and hs[i] % 5 in off:
                r0 = (hs[i] // 5) * n + off.index(hs[i] % 5)
            else:
                r0 = 0
            m.AddHint(r, r0)
            for j in range(d):
                q = m.NewIntVar(0, H // 5 + 2, f"q{i}_{p}_{j}")
                md = m.NewIntVar(0, n - 1, f"m{i}_{p}_{j}")
                o = m.NewIntVar(0, 4, f"o{i}_{p}_{j}")
                m.AddDivisionEquality(q, r + j, n)
                m.AddModuloEquality(md, r + j, n)
                m.AddElement(md, off, o)
                dj = m.NewIntVar(0, H + 5, f"d{i}_{p}_{j}")
                m.Add(dj == 5 * q + o)
                ds.append(dj)
                m.AddHint(q, (r0 + j) // n)
                m.AddHint(md, (r0 + j) % n)
                m.AddHint(o, off[(r0 + j) % n])
                m.AddHint(dj, 5 * ((r0 + j) // n) + off[(r0 + j) % n])
                if single:
                    loads[p].append((m.NewIntervalVar(dj, 1, dj + 1, f"u{i}_{p}_{j}"), h))
                else:
                    loads[p].append((m.NewOptionalIntervalVar(dj, 1, dj + 1, x, f"u{i}_{p}_{j}"), h))
            if single:
                m.Add(s[i] == ds[0])
                m.Add(e[i] == ds[-1] + 1)
            else:
                m.Add(s[i] == ds[0]).OnlyEnforceIf(x)
                m.Add(e[i] == ds[-1] + 1).OnlyEnforceIf(x)
    choice[i] = lits
    if not single:
        m.AddExactlyOne([x for _, x in lits])
for p in range(people):
    # a half day leaves half the hours, which another task cannot use
    for k in range(1, H):
        lost = 8 - caps[p][k % 5]
        if lost > 0:
            loads[p].append((m.NewFixedSizeIntervalVar(k, 1, f"half{p}_{k}"), lost))
    m.AddCumulative([iv for iv, _ in loads[p]], [h for _, h in loads[p]], 8)
for i in tasks:
    for j in deps[i]:
        m.Add(s[i] >= e[j])

cal = [cal_days(k) for k in range(H + 10)]
total_late = []
late_vars = {}
for i, t in tasks.items():
    if not t["constraint"]:
        continue
    kind, day = t["constraint"]
    when = (dt.date.fromisoformat(day) - base).days
    cs = m.NewIntVar(0, 2000, f"c{i}")
    m.AddElement(s[i], cal, cs)
    late = m.NewIntVar(0, 2000, f"l{i}")
    m.Add(late >= cs - when)
    total_late.append(late)
    late_vars[i] = (cs, late, when)
    if kind == "fixed":
        m.Add(s[i] >= index_of(dt.date.fromisoformat(day)))
end = m.NewIntVar(1, H + 8, "end")
for i in ends:
    m.Add(end >= e[i])
# days late first, then when the plan ends, then everything finishing early
m.Minimize(10**10 * sum(total_late) + 10**7 * end + sum(e.values()))
for i, v in hs.items():
    m.AddHint(s[i], v)
    m.AddHint(e[i], hf[i])
    for p, x in choice[i]:
        if x is not None:
            m.AddHint(x, 1 if str(hp.get(i)) == str(p) else 0)
for i, (cs, late, when) in late_vars.items():
    c = cal_days(hs[i])
    m.AddHint(cs, c)
    m.AddHint(late, max(0, c - when))
m.AddHint(end, max(hf[i] for i in ends))
solver = cp_model.CpSolver()
solver.parameters.max_time_in_seconds = seconds
solver.parameters.num_workers = workers
if os.environ.get('LNS'):
    solver.parameters.use_lns_only = True
solver.parameters.max_memory_in_mb = int(os.environ.get('MEM', '4500'))
if os.environ.get('LOG'):
    solver.parameters.log_search_progress = True

class Progress(cp_model.CpSolverSolutionCallback):
    def __init__(self):
        super().__init__()
        self.saved = 0
    def on_solution_callback(self):
        print(f"  t={self.WallTime():7.1f}s objective {self.ObjectiveValue():.0f} bound {self.BestObjectiveBound():.0f}", flush=True)
        # keep the best so far on disk, in case the run has to be stopped
        if self.WallTime() - self.saved > 60:
            self.saved = self.WallTime()
            chosen = {}
            for i, lits in choice.items():
                for p, x in lits:
                    if x is None or self.Value(x):
                        chosen[i] = p
            name = os.path.basename(path).replace("rich_", "solution_")
            json.dump({"starts": {i: date_of(self.Value(s[i])).isoformat() for i in tasks}, "people": chosen},
                      open(os.path.join(os.path.dirname(path), name), "w"))

status = solver.Solve(m, Progress())
h_all = sum(hf.values())
result = {"instance": path, "status": solver.StatusName(status), "seconds": solver.WallTime(),
          "heuristic": {"misses": h_miss, "late": h_late, "end_workdays": h_end, "last_workday": max(hf.values()), "mean_finish": h_all / len(hf)}}
if status in (cp_model.OPTIMAL, cp_model.FEASIBLE):
    starts = {i: solver.Value(s[i]) for i in tasks}
    miss, late, end_v = lateness(starts)
    ce = {i: solver.Value(e[i]) for i in tasks}
    result["cpsat"] = {"misses": miss, "late": late, "end_workdays": end_v, "last_workday": max(ce.values()), "mean_finish": sum(ce.values()) / len(ce),
                       "objective": solver.ObjectiveValue(), "bound": solver.BestObjectiveBound()}
    chosen = {}
    for i, lits in choice.items():
        for p, x in lits:
            if x is None or solver.Value(x):
                chosen[i] = p
    name = os.path.basename(path).replace("rich_", "solution_")
    json.dump({"starts": {i: date_of(k).isoformat() for i, k in starts.items()}, "people": chosen},
              open(os.path.join(os.path.dirname(path), name), "w"))
print(json.dumps(result))
open(path.replace(".json", ".result.json"), "w").write(json.dumps(result))
