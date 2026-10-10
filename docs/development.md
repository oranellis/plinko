# Development

## Running it

`./run_debug.sh` builds the debug client (`client/build.sh`) and server, and runs them. Open
http://127.0.0.1:3000 and log in as `admin@example.com` with `change-me-now`. The tenant is named
`Debug`, so its database is `data/Debug/tenant.db` in the repo, and the admin login is only used
when that folder is empty. Delete `data/Debug` to start again.

After a client change run the script again, or just `./client/build.sh` and refresh, since the
server reads the client's files from disk on each request. Needs the `wasm32-unknown-unknown`
target and `wasm-bindgen-cli`.

## Settings

The server reads the environment, and `run_debug.sh` passes these on if they are set.

- `PLINKO_TENANT` the tenant's name, always needed. Its files are in `data/<name>` under the folder
  the server runs from. A name is letters, digits, `-` and `_`.
- `PLINKO_ADMIN_EMAIL`, `PLINKO_ADMIN_PASSWORD` the first admin, read only when the tenant is made.
- `PLINKO_LISTEN` the address, default `0.0.0.0:3000` so every interface (set it to `127.0.0.1:3000` to keep it to this machine, or if the port is taken).
- `PLINKO_FILES` the client build, default `client/dist` under where it runs.
- `PLINKO_INSECURE_COOKIES=1` lets the session cookie go over plain HTTP. Never set it when hosting.

## Checks

- `./check.sh` clippy, all tests, and builds the model and client for the browser.
- `client/e2e/run.sh` browser checks against a throwaway tenant (needs node and chromium, set
  `CHROMIUM` if it is not on the PATH).
- `PLINKO_BENCH_SAVE=1 cargo run --release -p plinko --example bench` times scheduling 1000 tasks
  and saves that tenant as `data/Bench`. Run it with `PLINKO_TENANT=Bench` and log in as
  `root@localhost` with `password123` to try the client on a lot of work.
- `client/e2e/perf.sh [--release]` times hovering, panning, zooming and scrolling the chart, the list,
  allocations and the availability calendar on the `data/Bench` tenant (made as above), as the main
  thread's busy time per step and frame times. `PROFILE="gantt hover"` also lists where the script
  time went. Dev builds optimise dependencies and the client (see the root `Cargo.toml`), since
  unoptimised wasm made the chart lag.
- `cargo run --release -p plinko --example score [seeds] [plans]` scores how well the scheduler keeps
  hard dates on made up plans of 500 tasks with a tenth dated (misses, days late, plan end). Use it
  when changing how work is ordered. On 24 seeds of one plan, going from ordering by slack with a
  3 day margin to hard chains first by latest start took misses from 7.6 to 3.0 and days late from
  74 to 19.
