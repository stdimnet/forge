//! A tiny simulated debug adapter for tests and demos. It "runs" [`DEMO_SOURCE`], a small
//! C program, step by step and supports what Forge's DAP client uses: line, function,
//! exception and data breakpoints (with conditions, hit counts and logpoints), stepping
//! (also by instruction), threads, scopes and variables, evaluate, completions,
//! setVariable, goto, restart, memory, disassembly, registers, modules, loaded sources,
//! core dumps and the reverse requests `runInTerminal` and `startDebugging`.
//!
//! Launch arguments: `program`, `args`, `sourcePath` (where `demo.c` is; default next to
//! the program), `stopOnEntry`, `console: "integratedTerminal"` (asks the client to start
//! the program), `spawnChild` (asks for a child session), `crash` (SIGSEGV on line 19).
//! Attach arguments: `pid` or `coreFile`.

use std::collections::{BTreeMap, HashMap, HashSet};

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine as _;
use serde_json::{json, Value};
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};

use crate::codec::{encode, read_message};

/// The simulated program (`demo.c`).
pub const DEMO_SOURCE: &str = r#"#include <stdio.h>

struct point { int x; int y; };

int compute(int n) {
    int sum = 0;
    for (int i = 0; i < n; i++) {
        sum += i;
    }
    return sum;
}

int main(void) {
    int counter = 0;
    char name[8] = "forge";
    struct point origin = { 3, 4 };
    counter = compute(4);
    counter += origin.x;
    printf("%s %d\n", name, counter);
    return 0;
}
"#;

/// First line of `compute` (function breakpoints stop here).
const COMPUTE_ENTRY: u32 = 6;
const MAIN_THREAD: i64 = 1;
const WORKER_THREAD: i64 = 2;
const LOCALS: i64 = 1000;
const REGISTERS: i64 = 2000;
const ORIGIN: i64 = 3000;
const STACK_BASE: u64 = 0x7ffe_ef00;
const STACK_LEN: u64 = 0x200;
const COUNTER_ADDR: u64 = 0x7ffe_f000;
const NAME_ADDR: u64 = 0x7ffe_f008;
const ORIGIN_ADDR: u64 = 0x7ffe_f010;
const N_ADDR: u64 = 0x7ffe_efe0;
const SUM_ADDR: u64 = 0x7ffe_efe4;
const I_ADDR: u64 = 0x7ffe_efe8;

/// Machine code of a source line, 4 bytes per instruction.
fn line_code(line: u32) -> &'static [&'static str] {
    match line {
        5 => &[
            "push rbp",
            "mov rbp, rsp",
            "mov dword ptr [rbp - 0x14], edi",
        ],
        6 => &["mov dword ptr [rbp - 0x4], 0x0"],
        7 => &[
            "mov dword ptr [rbp - 0x8], 0x0",
            "mov eax, dword ptr [rbp - 0x8]",
            "cmp eax, dword ptr [rbp - 0x14]",
            "jge 0x401188",
        ],
        8 => &[
            "mov eax, dword ptr [rbp - 0x8]",
            "add dword ptr [rbp - 0x4], eax",
            "add dword ptr [rbp - 0x8], 0x1",
        ],
        10 => &["mov eax, dword ptr [rbp - 0x4]", "pop rbp", "ret"],
        13 => &["push rbp", "mov rbp, rsp", "sub rsp, 0x20"],
        14 => &["mov dword ptr [rbp - 0x4], 0x0"],
        15 => &["movabs rax, 0x6567726f66", "mov qword ptr [rbp - 0xc], rax"],
        16 => &[
            "mov dword ptr [rbp - 0x14], 0x3",
            "mov dword ptr [rbp - 0x10], 0x4",
        ],
        17 => &[
            "mov edi, 0x4",
            "call 0x401126 <compute>",
            "mov dword ptr [rbp - 0x4], eax",
        ],
        18 => &[
            "mov eax, dword ptr [rbp - 0x14]",
            "add dword ptr [rbp - 0x4], eax",
        ],
        19 => &[
            "mov edx, dword ptr [rbp - 0x4]",
            "lea rax, [rbp - 0xc]",
            "mov rsi, rax",
            "call 0x401030 <printf@plt>",
        ],
        20 => &["mov eax, 0x0"],
        21 => &["leave", "ret"],
        _ => &[],
    }
}

fn line_address(line: u32) -> u64 {
    0x40_1100 + u64::from(line) * 0x10
}

fn hex(a: u64) -> String {
    format!("0x{a:016x}")
}

#[derive(Debug, Clone, Default, PartialEq)]
struct Vars {
    counter: i64,
    name: String,
    origin: (i64, i64),
    /// `compute` locals: n, sum, and i inside the loop.
    n: Option<i64>,
    sum: Option<i64>,
    i: Option<i64>,
}

#[derive(Debug, Clone)]
struct Step {
    /// (function, line), top frame first.
    frames: Vec<(&'static str, u32)>,
    vars: Vars,
}

fn trace() -> Vec<Step> {
    let mut out = Vec::new();
    let mut v = Vars::default();
    let main = |line: u32, v: &Vars| Step {
        frames: vec![("main", line)],
        vars: v.clone(),
    };
    let compute = |line: u32, v: &Vars| Step {
        frames: vec![("compute", line), ("main", 17)],
        vars: v.clone(),
    };
    out.push(main(14, &v));
    out.push(main(15, &v));
    v.name = "forge".into();
    out.push(main(16, &v));
    v.origin = (3, 4);
    out.push(main(17, &v));
    v.n = Some(4);
    out.push(compute(6, &v));
    v.sum = Some(0);
    v.i = Some(0);
    out.push(compute(7, &v));
    for i in 0..4 {
        v.i = Some(i);
        out.push(compute(8, &v));
        v.sum = Some(v.sum.unwrap_or(0) + i);
        v.i = Some(i + 1);
        out.push(compute(7, &v));
    }
    v.i = None;
    out.push(compute(10, &v));
    let result = v.sum.unwrap_or(0);
    v.n = None;
    v.sum = None;
    v.counter = result;
    out.push(main(18, &v));
    v.counter += v.origin.0;
    out.push(main(19, &v));
    out.push(main(20, &v));
    out
}

#[derive(Debug, Clone)]
struct LineBp {
    id: i64,
    line: u32,
    condition: Option<String>,
    hit_condition: Option<String>,
    log_message: Option<String>,
    hits: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Mode {
    Continue,
    Next,
    StepIn,
    StepOut,
}

#[derive(Debug, Clone, PartialEq)]
enum Value2 {
    Int(i64),
    Str(String),
    Point(i64, i64),
    Addr(u64),
}

impl Value2 {
    fn display(&self) -> String {
        match self {
            Self::Int(i) => i.to_string(),
            Self::Str(s) => format!("\"{s}\""),
            Self::Point(x, y) => format!("{{x={x}, y={y}}}"),
            Self::Addr(a) => hex(*a),
        }
    }
    fn int(&self) -> Option<i64> {
        match self {
            Self::Int(i) => Some(*i),
            Self::Addr(a) => Some(*a as i64),
            _ => None,
        }
    }
}

/// The simulated adapter state.
pub struct FakeAdapter {
    seq: i64,
    out: Vec<Value>,
    steps: Vec<Step>,
    pc: usize,
    /// Instruction within the current line (instruction stepping).
    ip: usize,
    launched: bool,
    running: bool,
    terminated: bool,
    core_dump: bool,
    stop_on_entry: bool,
    crash: bool,
    console_terminal: bool,
    spawn_child: bool,
    program: String,
    program_args: Vec<String>,
    cwd: String,
    source_path: String,
    next_bp: i64,
    line_bps: Vec<LineBp>,
    function_bps: Vec<(i64, String)>,
    data_bps: Vec<(i64, String)>,
    filters: HashSet<String>,
    memory_writes: BTreeMap<u64, u8>,
    /// Reverse requests sent: seq → command.
    reverse: HashMap<i64, String>,
}

impl Default for FakeAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl FakeAdapter {
    pub fn new() -> Self {
        Self {
            seq: 1,
            out: Vec::new(),
            steps: trace(),
            pc: 0,
            ip: 0,
            launched: false,
            running: false,
            terminated: false,
            core_dump: false,
            stop_on_entry: false,
            crash: false,
            console_terminal: false,
            spawn_child: false,
            program: "demo".into(),
            program_args: Vec::new(),
            cwd: String::new(),
            source_path: "demo.c".into(),
            next_bp: 1,
            line_bps: Vec::new(),
            function_bps: Vec::new(),
            data_bps: Vec::new(),
            filters: HashSet::new(),
            memory_writes: BTreeMap::new(),
            reverse: HashMap::new(),
        }
    }

    fn next_seq(&mut self) -> i64 {
        let s = self.seq;
        self.seq += 1;
        s
    }

    fn event(&mut self, event: &str, body: Value) {
        let seq = self.next_seq();
        self.out
            .push(json!({"seq": seq, "type": "event", "event": event, "body": body}));
    }

    fn reverse_request(&mut self, command: &str, arguments: Value) {
        let seq = self.next_seq();
        self.reverse.insert(seq, command.to_string());
        self.out.push(
            json!({"seq": seq, "type": "request", "command": command, "arguments": arguments}),
        );
    }

    fn respond(&mut self, req: &Value, result: Result<Value, String>) {
        let seq = self.next_seq();
        let command = req["command"].clone();
        let request_seq = req["seq"].clone();
        self.out.push(match result {
            Ok(body) => json!({"seq": seq, "type": "response", "request_seq": request_seq,
                               "success": true, "command": command, "body": body}),
            Err(message) => json!({"seq": seq, "type": "response", "request_seq": request_seq,
                                   "success": false, "command": command, "message": message}),
        });
    }

    fn step(&self) -> &Step {
        &self.steps[self.pc]
    }

    fn top_line(&self) -> u32 {
        self.step().frames[0].1
    }

    fn current_address(&self) -> u64 {
        line_address(self.top_line()) + self.ip as u64 * 4
    }

    fn source(&self) -> Value {
        json!({"name": "demo.c", "path": self.source_path})
    }

    /// Handle one message from the client; returns the messages to send.
    pub fn handle(&mut self, msg: &Value) -> Vec<Value> {
        match msg["type"].as_str() {
            Some("request") => self.request(msg),
            Some("response") => {
                let seq = msg["request_seq"].as_i64().unwrap_or(0);
                if self.reverse.remove(&seq).as_deref() == Some("runInTerminal") {
                    let pid = msg["body"]["processId"].as_i64().unwrap_or(0);
                    self.event(
                        "output",
                        json!({"category": "console", "output": format!("Debuggee started in terminal (pid {pid})\n")}),
                    );
                }
            }
            _ => {}
        }
        std::mem::take(&mut self.out)
    }

    fn request(&mut self, req: &Value) {
        let args = &req["arguments"];
        let command = req["command"].as_str().unwrap_or_default().to_string();
        let result = match command.as_str() {
            "initialize" => Ok(capabilities()),
            "launch" | "attach" => self.launch(&command, args),
            "setBreakpoints" => Ok(self.set_breakpoints(args)),
            "setFunctionBreakpoints" => Ok(self.set_function_breakpoints(args)),
            "setExceptionBreakpoints" => {
                self.filters = args["filters"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|f| f.as_str().map(str::to_string))
                    .chain(
                        args["filterOptions"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(|f| f["filterId"].as_str().map(str::to_string)),
                    )
                    .collect();
                Ok(json!({}))
            }
            "dataBreakpointInfo" => Ok(self.data_breakpoint_info(args)),
            "setDataBreakpoints" => Ok(self.set_data_breakpoints(args)),
            "configurationDone" => {
                self.respond(req, Ok(json!({})));
                self.start_program();
                return;
            }
            "threads" => Ok(json!({"threads": [
                {"id": MAIN_THREAD, "name": "main"},
                {"id": WORKER_THREAD, "name": "worker"}
            ]})),
            "stackTrace" => Ok(self.stack_trace(args)),
            "scopes" => Ok(self.scopes(args)),
            "variables" => Ok(self.variables(args)),
            "evaluate" => self.evaluate(args),
            "completions" => Ok(self.completions(args)),
            "setVariable" => self.set_variable(args),
            "continue" | "next" | "stepIn" | "stepOut" => {
                if self.terminated {
                    Err("the program is not running".into())
                } else if self.core_dump {
                    Err("a core dump cannot run".into())
                } else {
                    self.respond(req, Ok(json!({"allThreadsContinued": true})));
                    let instruction = args["granularity"] == "instruction";
                    let mode = match command.as_str() {
                        "continue" => Mode::Continue,
                        "next" => Mode::Next,
                        "stepIn" => Mode::StepIn,
                        _ => Mode::StepOut,
                    };
                    if instruction && mode != Mode::Continue && mode != Mode::StepOut {
                        self.step_instruction();
                    } else {
                        self.run(mode);
                    }
                    return;
                }
            }
            "pause" => {
                if self.running && !self.terminated {
                    self.stopped("pause", None, None);
                }
                Ok(json!({}))
            }
            "gotoTargets" => {
                let line = args["line"].as_u64().unwrap_or(0) as u32;
                if line_code(line).is_empty() {
                    Ok(json!({"targets": []}))
                } else {
                    Ok(
                        json!({"targets": [{"id": line, "label": format!("demo.c:{line}"), "line": line}]}),
                    )
                }
            }
            "goto" => {
                self.goto(args, req);
                return;
            }
            "restart" => {
                self.respond(req, Ok(json!({})));
                self.pc = 0;
                self.ip = 0;
                for bp in &mut self.line_bps {
                    bp.hits = 0;
                }
                self.start_program();
                return;
            }
            "readMemory" => Ok(self.read_memory(args)),
            "writeMemory" => self.write_memory(args),
            "disassemble" => Ok(self.disassemble(args)),
            "modules" => Ok(json!({"modules": self.modules(), "totalModules": 2})),
            "loadedSources" => Ok(json!({"sources": [self.source()]})),
            "terminate" => {
                self.respond(req, Ok(json!({})));
                self.finish(143);
                return;
            }
            "disconnect" => Ok(json!({})),
            other => Err(format!("unsupported request '{other}'")),
        };
        self.respond(req, result);
    }

    fn launch(&mut self, command: &str, args: &Value) -> Result<Value, String> {
        self.launched = true;
        self.program = args["program"].as_str().unwrap_or("demo").to_string();
        self.program_args = args["args"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|a| a.as_str().map(str::to_string))
            .collect();
        self.cwd = args["cwd"].as_str().unwrap_or("").to_string();
        self.source_path = match args["sourcePath"].as_str() {
            Some(p) => p.to_string(),
            None => {
                let dir = std::path::Path::new(&self.program)
                    .parent()
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_default();
                if dir.is_empty() {
                    "demo.c".into()
                } else {
                    format!("{dir}/demo.c")
                }
            }
        };
        self.stop_on_entry = args["stopOnEntry"].as_bool().unwrap_or(false);
        self.crash = args["crash"].as_bool().unwrap_or(false);
        self.console_terminal = args["console"] == "integratedTerminal";
        self.spawn_child = args["spawnChild"].as_bool().unwrap_or(false);
        self.core_dump = command == "attach" && args.get("coreFile").is_some();
        let method = if command == "launch" {
            "launch"
        } else {
            "attach"
        };
        let pid = args["pid"].as_u64().unwrap_or(4242);
        self.event(
            "process",
            json!({"name": self.program, "systemProcessId": pid, "startMethod": method}),
        );
        for m in self.modules() {
            self.event("module", json!({"reason": "new", "module": m}));
        }
        self.event(
            "thread",
            json!({"reason": "started", "threadId": MAIN_THREAD}),
        );
        self.event(
            "thread",
            json!({"reason": "started", "threadId": WORKER_THREAD}),
        );
        self.event("initialized", json!({}));
        Ok(json!({}))
    }

    fn modules(&self) -> Vec<Value> {
        vec![
            json!({"id": 1, "name": "demo", "path": self.program, "symbolStatus": "Symbols loaded.", "addressRange": "0x0000000000401000"}),
            json!({"id": 2, "name": "libc.so.6", "path": "/usr/lib/x86_64-linux-gnu/libc.so.6", "symbolStatus": "Symbols not found.", "addressRange": "0x00007ffff7c00000"}),
        ]
    }

    fn start_program(&mut self) {
        if self.console_terminal {
            let mut cmd = vec![self.program.clone()];
            cmd.extend(self.program_args.clone());
            self.reverse_request(
                "runInTerminal",
                json!({"kind": "integrated", "title": "demo", "cwd": self.cwd, "args": cmd}),
            );
        }
        if self.spawn_child {
            self.reverse_request(
                "startDebugging",
                json!({"request": "launch", "configuration": {
                    "name": "demo (child)", "program": self.program,
                    "sourcePath": self.source_path, "stopOnEntry": true
                }}),
            );
        }
        self.running = true;
        if self.core_dump {
            self.pc = self
                .steps
                .iter()
                .position(|s| s.frames[0].1 == 19)
                .unwrap_or(0);
            self.stopped_with(
                "exception",
                Some("SIGSEGV"),
                Some("Core dump: segmentation fault"),
                None,
            );
        } else if self.stop_on_entry {
            self.stopped("entry", None, None);
        } else {
            // The first line can hold a breakpoint as well.
            if let Some(stop) = self.check_line_breakpoints() {
                self.stopped("breakpoint", None, Some(vec![stop]));
            } else {
                self.run(Mode::Continue);
            }
        }
    }

    fn stopped(&mut self, reason: &str, description: Option<&str>, hits: Option<Vec<i64>>) {
        self.stopped_with(reason, description, None, hits);
    }

    fn stopped_with(
        &mut self,
        reason: &str,
        description: Option<&str>,
        text: Option<&str>,
        hits: Option<Vec<i64>>,
    ) {
        self.running = false;
        let mut body =
            json!({"reason": reason, "threadId": MAIN_THREAD, "allThreadsStopped": true});
        if let Some(d) = description {
            body["description"] = json!(d);
        }
        if let Some(t) = text {
            body["text"] = json!(t);
        }
        if let Some(h) = hits {
            body["hitBreakpointIds"] = json!(h);
        }
        self.event("stopped", body);
    }

    fn finish(&mut self, code: i64) {
        if self.terminated {
            return;
        }
        self.terminated = true;
        self.running = false;
        self.event("exited", json!({"exitCode": code}));
        self.event("terminated", json!({}));
    }

    /// Advance until something stops the program.
    fn run(&mut self, mode: Mode) {
        self.running = true;
        let start_depth = self.step().frames.len();
        loop {
            let prev = self.pc;
            if self.top_line() == 19 && !self.core_dump {
                if self.crash {
                    if self.filters.contains("signal") {
                        self.stopped_with(
                            "exception",
                            Some("SIGSEGV"),
                            Some("Segmentation fault"),
                            None,
                        );
                    } else {
                        self.event("output", json!({"category": "stderr", "output": "Segmentation fault (core dumped)\n"}));
                        self.finish(139);
                    }
                    return;
                }
                let v = &self.step().vars;
                let text = format!("{} {}\n", v.name, v.counter);
                self.event("output", json!({"category": "stdout", "output": text}));
            }
            if self.pc + 1 >= self.steps.len() {
                self.finish(0);
                return;
            }
            self.pc += 1;
            self.ip = 0;
            let depth = self.step().frames.len();
            // Watchpoints.
            for (id, data_id) in self.data_bps.clone() {
                let before = watched(&self.steps[prev].vars, &data_id);
                let after = watched(&self.step().vars, &data_id);
                if before.is_some() && after.is_some() && before != after {
                    let desc = format!("{} changed", data_id.trim_start_matches('&'));
                    self.stopped("data breakpoint", Some(&desc), Some(vec![id]));
                    return;
                }
            }
            // Function breakpoints.
            if depth > self.steps[prev].frames.len() && self.top_line() == COMPUTE_ENTRY {
                if let Some((id, _)) = self.function_bps.iter().find(|(_, n)| n == "compute") {
                    let id = *id;
                    self.stopped("function breakpoint", None, Some(vec![id]));
                    return;
                }
            }
            if let Some(id) = self.check_line_breakpoints() {
                self.stopped("breakpoint", None, Some(vec![id]));
                return;
            }
            let stop = match mode {
                Mode::Continue => false,
                Mode::StepIn => true,
                Mode::Next => depth <= start_depth,
                Mode::StepOut => depth < start_depth,
            };
            if stop {
                self.stopped("step", None, None);
                return;
            }
        }
    }

    /// One machine instruction: the next instruction of the line, else the next line.
    fn step_instruction(&mut self) {
        if self.ip + 1 < line_code(self.top_line()).len() {
            self.ip += 1;
            self.stopped("step", None, None);
        } else {
            self.run(Mode::StepIn);
        }
    }

    /// Breakpoint at the current line that stops (logpoints log and run on).
    fn check_line_breakpoints(&mut self) -> Option<i64> {
        let line = self.top_line();
        let mut stop = None;
        for idx in 0..self.line_bps.len() {
            if self.line_bps[idx].line != line {
                continue;
            }
            if let Some(cond) = self.line_bps[idx].condition.clone() {
                let truthy = self.eval(&cond, 0).ok().and_then(|v| v.int()).unwrap_or(0) != 0;
                if !truthy {
                    continue;
                }
            }
            self.line_bps[idx].hits += 1;
            let bp = self.line_bps[idx].clone();
            if let Some(n) = bp
                .hit_condition
                .as_deref()
                .and_then(|h| h.trim_start_matches(">=").trim().parse::<u32>().ok())
            {
                if bp.hits < n {
                    continue;
                }
            }
            if let Some(msg) = &bp.log_message {
                let text = self.interpolate(msg);
                self.event(
                    "output",
                    json!({"category": "console", "output": format!("{text}\n")}),
                );
                continue;
            }
            stop = Some(bp.id);
        }
        stop
    }

    fn interpolate(&self, template: &str) -> String {
        let mut out = String::new();
        let mut rest = template;
        while let Some(start) = rest.find('{') {
            out.push_str(&rest[..start]);
            let after = &rest[start + 1..];
            let Some(end) = after.find('}') else {
                out.push_str(&rest[start..]);
                return out;
            };
            match self.eval(&after[..end], 0) {
                Ok(v) => out.push_str(&v.display()),
                Err(e) => out.push_str(&format!("<{e}>")),
            }
            rest = &after[end + 1..];
        }
        out.push_str(rest);
        out
    }

    fn set_breakpoints(&mut self, args: &Value) -> Value {
        let mut answered = Vec::new();
        let mut list = Vec::new();
        for b in args["breakpoints"].as_array().into_iter().flatten() {
            let requested = b["line"].as_u64().unwrap_or(0) as u32;
            // Lines without code move to the next line with code.
            let line = (requested..=21).find(|l| !line_code(*l).is_empty() && *l != 5 && *l != 13);
            let id = self.next_bp;
            self.next_bp += 1;
            match line {
                Some(line) => {
                    list.push(LineBp {
                        id,
                        line,
                        condition: b["condition"].as_str().map(str::to_string),
                        hit_condition: b["hitCondition"].as_str().map(str::to_string),
                        log_message: b["logMessage"].as_str().map(str::to_string),
                        hits: 0,
                    });
                    answered.push(
                        json!({"id": id, "verified": true, "line": line, "source": self.source()}),
                    );
                }
                None => answered
                    .push(json!({"id": id, "verified": false, "message": "No code at this line"})),
            }
        }
        self.line_bps = list;
        json!({"breakpoints": answered})
    }

    fn set_function_breakpoints(&mut self, args: &Value) -> Value {
        let mut answered = Vec::new();
        self.function_bps.clear();
        for b in args["breakpoints"].as_array().into_iter().flatten() {
            let name = b["name"].as_str().unwrap_or_default().to_string();
            let id = self.next_bp;
            self.next_bp += 1;
            let known = name == "compute" || name == "main";
            if known {
                self.function_bps.push((id, name));
            }
            answered.push(json!({"id": id, "verified": known}));
        }
        json!({"breakpoints": answered})
    }

    fn data_breakpoint_info(&self, args: &Value) -> Value {
        let name = args["name"].as_str().unwrap_or_default();
        let reference = args["variablesReference"].as_i64().unwrap_or(0);
        let scalar = ["counter", "n", "sum", "i"].contains(&name);
        if (LOCALS..LOCALS + 10).contains(&reference) && scalar {
            json!({"dataId": format!("&{name}"), "description": name, "accessTypes": ["write"], "canPersist": true})
        } else {
            json!({"dataId": null, "description": format!("'{name}' is not a scalar local variable")})
        }
    }

    fn set_data_breakpoints(&mut self, args: &Value) -> Value {
        let mut answered = Vec::new();
        self.data_bps.clear();
        for b in args["breakpoints"].as_array().into_iter().flatten() {
            let id = self.next_bp;
            self.next_bp += 1;
            self.data_bps
                .push((id, b["dataId"].as_str().unwrap_or_default().to_string()));
            answered.push(json!({"id": id, "verified": true}));
        }
        json!({"breakpoints": answered})
    }

    fn stack_trace(&self, args: &Value) -> Value {
        if args["threadId"] == WORKER_THREAD {
            return json!({"stackFrames": [
                {"id": 51, "name": "__futex_abstimed_wait_common", "line": 0, "column": 0,
                 "instructionPointerReference": "0x00007ffff7c91117", "presentationHint": "subtle"},
                {"id": 52, "name": "worker_loop", "line": 0, "column": 0,
                 "instructionPointerReference": "0x00007ffff7c9a2c4"}
            ], "totalFrames": 2});
        }
        let frames: Vec<Value> = self
            .step()
            .frames
            .iter()
            .enumerate()
            .map(|(level, (func, line))| {
                let ip = if level == 0 {
                    self.current_address()
                } else {
                    line_address(*line) + 4
                };
                json!({"id": level + 1, "name": func, "source": self.source(), "line": line,
                       "column": 5, "instructionPointerReference": hex(ip), "moduleId": 1})
            })
            .collect();
        let total = frames.len();
        json!({"stackFrames": frames, "totalFrames": total})
    }

    fn level_of(&self, frame_id: i64) -> Option<usize> {
        let level = usize::try_from(frame_id - 1).ok()?;
        (level < self.step().frames.len()).then_some(level)
    }

    fn scopes(&self, args: &Value) -> Value {
        let Some(level) = self.level_of(args["frameId"].as_i64().unwrap_or(0)) else {
            return json!({"scopes": []});
        };
        let level = level as i64;
        json!({"scopes": [
            {"name": "Locals", "presentationHint": "locals", "variablesReference": LOCALS + level, "expensive": false},
            {"name": "Registers", "presentationHint": "registers", "variablesReference": REGISTERS + level, "expensive": false}
        ]})
    }

    fn locals(&self, level: usize) -> Vec<(String, Value2, &'static str, u64)> {
        let v = &self.step().vars;
        match self.step().frames.get(level).map(|f| f.0) {
            Some("compute") => {
                let mut out = vec![
                    (
                        "n".to_string(),
                        Value2::Int(v.n.unwrap_or(0)),
                        "int",
                        N_ADDR,
                    ),
                    (
                        "sum".to_string(),
                        Value2::Int(v.sum.unwrap_or(0)),
                        "int",
                        SUM_ADDR,
                    ),
                ];
                if let Some(i) = v.i {
                    out.push(("i".to_string(), Value2::Int(i), "int", I_ADDR));
                }
                out
            }
            _ => vec![
                (
                    "counter".to_string(),
                    Value2::Int(v.counter),
                    "int",
                    COUNTER_ADDR,
                ),
                (
                    "name".to_string(),
                    Value2::Str(v.name.clone()),
                    "char[8]",
                    NAME_ADDR,
                ),
                (
                    "origin".to_string(),
                    Value2::Point(v.origin.0, v.origin.1),
                    "struct point",
                    ORIGIN_ADDR,
                ),
            ],
        }
    }

    fn registers(&self, level: usize) -> Vec<(String, String)> {
        let v = &self.step().vars;
        let in_compute = self.step().frames.get(level).map(|f| f.0) == Some("compute");
        let rsp: u64 = if in_compute { 0x7ffe_efd0 } else { 0x7ffe_eff0 };
        let rip = if level == 0 {
            self.current_address()
        } else {
            line_address(self.step().frames[level].1) + 4
        };
        let rax = if in_compute {
            v.sum.unwrap_or(0)
        } else {
            v.counter
        };
        vec![
            ("rax".into(), hex(rax as u64)),
            ("rbx".into(), hex(0)),
            ("rsp".into(), hex(rsp)),
            ("rbp".into(), hex(rsp + 0x20)),
            ("rip".into(), hex(rip)),
            ("eflags".into(), "0x00000246".into()),
        ]
    }

    fn variables(&self, args: &Value) -> Value {
        let reference = args["variablesReference"].as_i64().unwrap_or(0);
        let vars: Vec<Value> = if (LOCALS..LOCALS + 10).contains(&reference) {
            let level = (reference - LOCALS) as usize;
            self.locals(level)
                .into_iter()
                .map(|(name, value, ty, addr)| {
                    let children = matches!(value, Value2::Point(..));
                    json!({"name": name, "value": value.display(), "type": ty,
                           "evaluateName": name,
                           "variablesReference": if children { ORIGIN + level as i64 } else { 0 },
                           "namedVariables": if children { 2 } else { 0 },
                           "memoryReference": hex(addr)})
                })
                .collect()
        } else if (REGISTERS..REGISTERS + 10).contains(&reference) {
            self.registers((reference - REGISTERS) as usize)
                .into_iter()
                .map(|(name, value)| json!({"name": name, "value": value, "type": "unsigned long", "variablesReference": 0}))
                .collect()
        } else if (ORIGIN..ORIGIN + 10).contains(&reference) {
            let (x, y) = self.step().vars.origin;
            vec![
                json!({"name": "x", "value": x.to_string(), "type": "int", "evaluateName": "origin.x", "variablesReference": 0, "memoryReference": hex(ORIGIN_ADDR)}),
                json!({"name": "y", "value": y.to_string(), "type": "int", "evaluateName": "origin.y", "variablesReference": 0, "memoryReference": hex(ORIGIN_ADDR + 4)}),
            ]
        } else {
            Vec::new()
        };
        json!({"variables": vars})
    }

    fn eval(&self, expr: &str, level: usize) -> Result<Value2, String> {
        let expr = expr.trim();
        for op in ["==", "!=", ">=", "<=", ">", "<", "+", "-", "*"] {
            if let Some(pos) = expr.find(op).filter(|p| *p > 0) {
                let l = self
                    .eval(&expr[..pos], level)?
                    .int()
                    .ok_or("not a number")?;
                let r = self
                    .eval(&expr[pos + op.len()..], level)?
                    .int()
                    .ok_or("not a number")?;
                return Ok(Value2::Int(match op {
                    "==" => (l == r) as i64,
                    "!=" => (l != r) as i64,
                    ">=" => (l >= r) as i64,
                    "<=" => (l <= r) as i64,
                    ">" => (l > r) as i64,
                    "<" => (l < r) as i64,
                    "+" => l + r,
                    "-" => l - r,
                    _ => l * r,
                }));
            }
        }
        if let Ok(n) = expr.parse::<i64>() {
            return Ok(Value2::Int(n));
        }
        let locals = self.locals(level);
        if let Some(name) = expr.strip_prefix('&') {
            return locals
                .iter()
                .find(|(n, ..)| n == name)
                .map(|(.., addr)| Value2::Addr(*addr))
                .ok_or_else(|| format!("use of undeclared identifier '{name}'"));
        }
        let (base, field) = match expr.split_once('.') {
            Some((b, f)) => (b, Some(f)),
            None => (expr, None),
        };
        let value = locals
            .iter()
            .find(|(n, ..)| n == base)
            .map(|(_, v, ..)| v.clone())
            .ok_or_else(|| format!("use of undeclared identifier '{base}'"))?;
        match (value, field) {
            (Value2::Point(x, _), Some("x")) => Ok(Value2::Int(x)),
            (Value2::Point(_, y), Some("y")) => Ok(Value2::Int(y)),
            (_, Some(f)) => Err(format!("no member named '{f}'")),
            (v, None) => Ok(v),
        }
    }

    fn evaluate(&self, args: &Value) -> Result<Value, String> {
        let level = self
            .level_of(args["frameId"].as_i64().unwrap_or(1))
            .unwrap_or(0);
        let expr = args["expression"].as_str().unwrap_or_default();
        let v = self.eval(expr, level)?;
        let reference = if matches!(v, Value2::Point(..)) {
            ORIGIN + level as i64
        } else {
            0
        };
        let ty = match v {
            Value2::Int(_) => "int",
            Value2::Str(_) => "char[8]",
            Value2::Point(..) => "struct point",
            Value2::Addr(_) => "int *",
        };
        let mut body = json!({"result": v.display(), "type": ty, "variablesReference": reference});
        if let Value2::Addr(a) = v {
            body["memoryReference"] = json!(hex(a));
        }
        Ok(body)
    }

    fn completions(&self, args: &Value) -> Value {
        let text = args["text"].as_str().unwrap_or_default();
        let column = args["column"].as_u64().unwrap_or(1).max(1) as usize - 1;
        let upto: String = text.chars().take(column).collect();
        let prefix: String = upto
            .chars()
            .rev()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        let level = self
            .level_of(args["frameId"].as_i64().unwrap_or(1))
            .unwrap_or(0);
        let targets: Vec<Value> = self
            .locals(level)
            .into_iter()
            .filter(|(n, ..)| n.starts_with(&prefix))
            .map(|(n, _, ty, _)| json!({"label": n, "type": "variable", "detail": ty}))
            .collect();
        json!({"targets": targets})
    }

    fn set_variable(&mut self, args: &Value) -> Result<Value, String> {
        let name = args["name"].as_str().unwrap_or_default();
        let value: i64 = args["value"]
            .as_str()
            .unwrap_or_default()
            .trim()
            .parse()
            .map_err(|_| "expected an integer".to_string())?;
        let pc = self.pc;
        let delta = {
            let v = &mut self.steps[pc].vars;
            match name {
                "counter" => {
                    let d = value - v.counter;
                    v.counter = value;
                    d
                }
                "sum" if v.sum.is_some() => {
                    v.sum = Some(value);
                    0
                }
                "n" if v.n.is_some() => {
                    v.n = Some(value);
                    0
                }
                _ => return Err(format!("cannot set '{name}'")),
            }
        };
        if delta != 0 {
            for s in &mut self.steps[pc + 1..] {
                s.vars.counter += delta;
            }
        }
        Ok(json!({"value": value.to_string(), "type": "int"}))
    }

    fn goto(&mut self, args: &Value, req: &Value) {
        let line = args["targetId"].as_u64().unwrap_or(0) as u32;
        let depth = self.step().frames.len();
        let func = self.step().frames[0].0;
        let found = (self.pc..self.steps.len())
            .chain((0..self.pc).rev())
            .find(|&i| {
                let s = &self.steps[i];
                s.frames.len() == depth && s.frames[0] == (func, line)
            });
        let Some(pc) = found else {
            self.respond(req, Err(format!("cannot jump to line {line}")));
            return;
        };
        self.respond(req, Ok(json!({})));
        self.pc = pc;
        self.ip = 0;
        self.stopped("goto", None, None);
    }

    /// Bytes of the simulated stack: locals of the current step, then client writes.
    fn memory_byte(&self, addr: u64) -> Option<u8> {
        if !(STACK_BASE..STACK_BASE + STACK_LEN).contains(&addr) {
            return None;
        }
        if let Some(b) = self.memory_writes.get(&addr) {
            return Some(*b);
        }
        let v = &self.step().vars;
        let int_at = |base: u64, value: i64| -> Option<u8> {
            (base..base + 4)
                .contains(&addr)
                .then(|| (value as i32).to_le_bytes()[(addr - base) as usize])
        };
        let mut name = [0u8; 8];
        for (i, b) in v.name.bytes().take(8).enumerate() {
            name[i] = b;
        }
        int_at(COUNTER_ADDR, v.counter)
            .or_else(|| {
                (NAME_ADDR..NAME_ADDR + 8)
                    .contains(&addr)
                    .then(|| name[(addr - NAME_ADDR) as usize])
            })
            .or_else(|| int_at(ORIGIN_ADDR, v.origin.0))
            .or_else(|| int_at(ORIGIN_ADDR + 4, v.origin.1))
            .or_else(|| v.n.and_then(|n| int_at(N_ADDR, n)))
            .or_else(|| v.sum.and_then(|s| int_at(SUM_ADDR, s)))
            .or_else(|| v.i.and_then(|i| int_at(I_ADDR, i)))
            .or(Some(if addr % 16 == 8 { 0x7f } else { 0 }))
    }

    fn read_memory(&self, args: &Value) -> Value {
        let base = crate::views::parse_address(args["memoryReference"].as_str().unwrap_or("0"))
            .unwrap_or(0);
        let start = base.wrapping_add_signed(args["offset"].as_i64().unwrap_or(0));
        let count = args["count"].as_u64().unwrap_or(0).min(4096);
        let mut data = Vec::new();
        for a in start..start + count {
            match self.memory_byte(a) {
                Some(b) => data.push(b),
                None => break,
            }
        }
        let unreadable = count - data.len() as u64;
        json!({"address": hex(start), "data": B64.encode(&data), "unreadableBytes": unreadable})
    }

    fn write_memory(&mut self, args: &Value) -> Result<Value, String> {
        let base = crate::views::parse_address(args["memoryReference"].as_str().unwrap_or("0"))
            .unwrap_or(0);
        let start = base.wrapping_add_signed(args["offset"].as_i64().unwrap_or(0));
        let data = B64
            .decode(args["data"].as_str().unwrap_or_default())
            .map_err(|e| e.to_string())?;
        if self.memory_byte(start).is_none() {
            return Err(format!("cannot write memory at {}", hex(start)));
        }
        for (i, b) in data.iter().enumerate() {
            self.memory_writes.insert(start + i as u64, *b);
        }
        let n = data.len();
        self.event(
            "memory",
            json!({"memoryReference": hex(start), "offset": 0, "count": n}),
        );
        Ok(json!({"bytesWritten": n}))
    }

    fn disassemble(&self, args: &Value) -> Value {
        let mut all: Vec<(u64, u32, &str)> = Vec::new();
        for line in 1..=21 {
            for (k, ins) in line_code(line).iter().enumerate() {
                all.push((line_address(line) + k as u64 * 4, line, ins));
            }
        }
        let start = crate::views::parse_address(args["memoryReference"].as_str().unwrap_or("0"))
            .unwrap_or(0)
            .wrapping_add_signed(args["offset"].as_i64().unwrap_or(0));
        let anchor = all
            .iter()
            .position(|(a, ..)| *a >= start)
            .unwrap_or(all.len()) as i64;
        let first = anchor + args["instructionOffset"].as_i64().unwrap_or(0);
        let count = args["instructionCount"]
            .as_i64()
            .unwrap_or(0)
            .clamp(0, 1000);
        let mut last_line = 0;
        let list: Vec<Value> = (first..first + count)
            .map(|i| match usize::try_from(i).ok().and_then(|i| all.get(i)) {
                Some((addr, line, ins)) => {
                    let symbol = if *line >= 13 { "main" } else { "compute" };
                    let mut v = json!({"address": hex(*addr), "instruction": ins,
                                       "instructionBytes": format!("{:02x} {:02x} {:02x} {:02x}", addr & 0xff, line, ins.len(), 0x90),
                                       "symbol": format!("{symbol}+{}", addr - line_address(if *line >= 13 { 13 } else { 5 })),
                                       "line": line});
                    if *line != last_line {
                        v["location"] = self.source();
                        last_line = *line;
                    }
                    v
                }
                None => json!({"address": hex(0x40_1000u64.wrapping_add((i.max(0) as u64) * 4)), "instruction": "??", "presentationHint": "invalid"}),
            })
            .collect();
        json!({"instructions": list})
    }
}

/// Value a data breakpoint watches.
fn watched(v: &Vars, data_id: &str) -> Option<i64> {
    match data_id.trim_start_matches('&') {
        "counter" => Some(v.counter),
        "sum" => v.sum,
        "n" => v.n,
        "i" => v.i,
        _ => None,
    }
}

fn capabilities() -> Value {
    json!({
        "supportsConfigurationDoneRequest": true,
        "supportsFunctionBreakpoints": true,
        "supportsConditionalBreakpoints": true,
        "supportsHitConditionalBreakpoints": true,
        "supportsEvaluateForHovers": true,
        "supportsSetVariable": true,
        "supportsGotoTargetsRequest": true,
        "supportsCompletionsRequest": true,
        "supportsModulesRequest": true,
        "supportsRestartRequest": true,
        "supportsTerminateRequest": true,
        "supportsDataBreakpoints": true,
        "supportsReadMemoryRequest": true,
        "supportsWriteMemoryRequest": true,
        "supportsDisassembleRequest": true,
        "supportsLoadedSourcesRequest": true,
        "supportsLogPoints": true,
        "supportsSteppingGranularity": true,
        "exceptionBreakpointFilters": [
            {"filter": "signal", "label": "Signals (SIGSEGV, SIGABRT)", "default": true},
            {"filter": "cpp_throw", "label": "C++: on throw", "default": false}
        ]
    })
}

/// Serve one client on `reader`/`writer` until it disconnects.
pub async fn serve<R, W>(reader: R, mut writer: W) -> std::io::Result<()>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut reader = BufReader::new(reader);
    let mut adapter = FakeAdapter::new();
    loop {
        let msg = match read_message(&mut reader).await {
            Ok(Some(m)) => m,
            Ok(None) => return Ok(()),
            Err(e) => return Err(std::io::Error::other(e.to_string())),
        };
        let disconnect = msg["command"] == "disconnect";
        for out in adapter.handle(&msg) {
            writer.write_all(&encode(&out)).await?;
        }
        writer.flush().await?;
        if disconnect {
            return Ok(());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_trace_runs_the_program() {
        let t = trace();
        assert_eq!(t.first().unwrap().frames[0], ("main", 14));
        let last = t.last().unwrap();
        assert_eq!(last.frames[0], ("main", 20));
        assert_eq!(last.vars.counter, 9);
        assert!(t.iter().any(|s| s.frames.len() == 2 && s.vars.i == Some(3)));
        for s in &t {
            assert!(
                !line_code(s.frames[0].1).is_empty(),
                "line {} has code",
                s.frames[0].1
            );
        }
        assert!(DEMO_SOURCE.lines().nth(16).unwrap().contains("compute(4)"));
    }

    #[test]
    fn evaluates_simple_c_expressions() {
        let mut a = FakeAdapter::new();
        a.pc = trace()
            .iter()
            .position(|s| s.frames[0] == ("main", 18))
            .unwrap();
        assert_eq!(a.eval("counter", 0), Ok(Value2::Int(6)));
        assert_eq!(a.eval("origin.x + 1", 0), Ok(Value2::Int(4)));
        assert_eq!(a.eval("counter > 5", 0), Ok(Value2::Int(1)));
        assert_eq!(a.eval("&counter", 0), Ok(Value2::Addr(COUNTER_ADDR)));
        assert!(a.eval("nope", 0).is_err());
        assert_eq!(a.interpolate("c={counter} n={name}"), "c=6 n=\"forge\"");
        assert_eq!(a.memory_byte(COUNTER_ADDR), Some(6));
        assert_eq!(a.memory_byte(NAME_ADDR), Some(b'f'));
        assert_eq!(a.memory_byte(0x10), None);
    }
}
