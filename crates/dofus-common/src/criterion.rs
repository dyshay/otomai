//! Dofus criterion string parser and evaluator.
//!
//! Format: "Qf=42&PL>10|(Ps=1)"
//! - `&` = AND, `|` = OR, `()` for grouping
//! - Each atom: 2-letter type + operator (=, !=, <, >, <=, >=) + value
//!
//! Criterion types (from ItemCriterion.as subclasses):
//! - Qf: Quest finished (value = quest_id)
//! - Qa: Quest active (value = quest_id)
//! - Qc: Quest startable (value = quest_id) — always true server-side
//! - PL: Player level
//! - Ps: Breed (class)
//! - PS: Sex (0=male, 1=female)
//! - Pb: Subscribed (always true for private server)

/// Context needed to evaluate criteria.
pub struct CriterionContext {
    pub level: i32,
    pub breed_id: i32,
    pub sex: i32,
    pub completed_quest_ids: Vec<i32>,
    pub active_quest_ids: Vec<i32>,
}

/// Evaluate a criterion string against a player context.
/// Returns true if the criterion is satisfied (or empty/unparseable).
pub fn evaluate(criterion: &str, ctx: &CriterionContext) -> bool {
    let s = criterion.trim();
    if s.is_empty() {
        return true;
    }
    match parse_group(s) {
        Some((node, _)) => eval_node(&node, ctx),
        None => true, // Unparseable = pass (lenient)
    }
}

// ─── AST ───────────────────────────────────────────────────────────

#[derive(Debug)]
enum Node {
    Atom(Atom),
    And(Box<Node>, Box<Node>),
    Or(Box<Node>, Box<Node>),
}

#[derive(Debug)]
struct Atom {
    criterion_type: String, // "Qf", "PL", etc.
    operator: Op,
    value: i32,
}

#[derive(Debug, PartialEq)]
enum Op {
    Eq,
    NotEq,
    Lt,
    Gt,
    LtEq,
    GtEq,
}

// ─── Parser ────────────────────────────────────────────────────────

fn parse_group(s: &str) -> Option<(Node, &str)> {
    let (mut left, mut rest) = parse_term(s)?;

    loop {
        rest = rest.trim_start();
        if rest.starts_with('|') {
            let (right, r) = parse_term(&rest[1..])?;
            left = Node::Or(Box::new(left), Box::new(right));
            rest = r;
        } else {
            break;
        }
    }

    Some((left, rest))
}

fn parse_term(s: &str) -> Option<(Node, &str)> {
    let (mut left, mut rest) = parse_factor(s)?;

    loop {
        rest = rest.trim_start();
        if rest.starts_with('&') {
            let (right, r) = parse_factor(&rest[1..])?;
            left = Node::And(Box::new(left), Box::new(right));
            rest = r;
        } else {
            break;
        }
    }

    Some((left, rest))
}

fn parse_factor(s: &str) -> Option<(Node, &str)> {
    let s = s.trim_start();
    if s.starts_with('(') {
        let (node, rest) = parse_group(&s[1..])?;
        let rest = rest.trim_start().strip_prefix(')')?;
        Some((node, rest))
    } else {
        parse_atom(s)
    }
}

fn parse_atom(s: &str) -> Option<(Node, &str)> {
    let s = s.trim_start();
    if s.len() < 3 {
        return None;
    }

    // Type is first 2 chars
    let criterion_type = &s[..2];

    // Find operator start
    let rest = &s[2..];
    let (op, op_len) = if rest.starts_with("!=") {
        (Op::NotEq, 2)
    } else if rest.starts_with("<=") {
        (Op::LtEq, 2)
    } else if rest.starts_with(">=") {
        (Op::GtEq, 2)
    } else if rest.starts_with('=') {
        (Op::Eq, 1)
    } else if rest.starts_with('<') {
        (Op::Lt, 1)
    } else if rest.starts_with('>') {
        (Op::Gt, 1)
    } else {
        return None;
    };

    let value_str = &rest[op_len..];

    // Parse value: digits until non-digit
    let value_end = value_str
        .find(|c: char| !c.is_ascii_digit() && c != '-')
        .unwrap_or(value_str.len());

    if value_end == 0 {
        return None;
    }

    let value: i32 = value_str[..value_end].parse().ok()?;
    let remaining = &value_str[value_end..];

    Some((
        Node::Atom(Atom {
            criterion_type: criterion_type.to_string(),
            operator: op,
            value,
        }),
        remaining,
    ))
}

// ─── Evaluator ─────────────────────────────────────────────────────

fn eval_node(node: &Node, ctx: &CriterionContext) -> bool {
    match node {
        Node::Atom(atom) => eval_atom(atom, ctx),
        Node::And(left, right) => eval_node(left, ctx) && eval_node(right, ctx),
        Node::Or(left, right) => eval_node(left, ctx) || eval_node(right, ctx),
    }
}

fn eval_atom(atom: &Atom, ctx: &CriterionContext) -> bool {
    let actual = match atom.criterion_type.as_str() {
        "PL" => ctx.level,
        "Ps" => ctx.breed_id,
        "PS" => ctx.sex,
        "Qf" => {
            // Quest finished: check if quest_id is in completed list
            let has = ctx.completed_quest_ids.contains(&atom.value);
            return match atom.operator {
                Op::Eq => has,
                Op::NotEq => !has,
                _ => has,
            };
        }
        "Qa" => {
            // Quest active
            let has = ctx.active_quest_ids.contains(&atom.value);
            return match atom.operator {
                Op::Eq => has,
                Op::NotEq => !has,
                _ => has,
            };
        }
        "Qc" => return true, // Quest startable — always true server-side
        "Pb" => return true, // Subscribed — always true on private server
        _ => return true,    // Unknown criterion — pass (lenient)
    };

    compare(actual, &atom.operator, atom.value)
}

fn compare(actual: i32, op: &Op, expected: i32) -> bool {
    match op {
        Op::Eq => actual == expected,
        Op::NotEq => actual != expected,
        Op::Lt => actual < expected,
        Op::Gt => actual > expected,
        Op::LtEq => actual <= expected,
        Op::GtEq => actual >= expected,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(level: i32, breed: i32, completed: &[i32], active: &[i32]) -> CriterionContext {
        CriterionContext {
            level,
            breed_id: breed,
            sex: 0,
            completed_quest_ids: completed.to_vec(),
            active_quest_ids: active.to_vec(),
        }
    }

    #[test]
    fn empty_criterion() {
        assert!(evaluate("", &ctx(1, 1, &[], &[])));
    }

    #[test]
    fn level_check() {
        assert!(evaluate("PL>5", &ctx(10, 1, &[], &[])));
        assert!(!evaluate("PL>5", &ctx(3, 1, &[], &[])));
        assert!(evaluate("PL=10", &ctx(10, 1, &[], &[])));
    }

    #[test]
    fn breed_check() {
        assert!(evaluate("Ps=8", &ctx(1, 8, &[], &[]))); // Iop
        assert!(!evaluate("Ps=8", &ctx(1, 3, &[], &[])));
    }

    #[test]
    fn quest_finished() {
        assert!(evaluate("Qf=42", &ctx(1, 1, &[42], &[])));
        assert!(!evaluate("Qf=42", &ctx(1, 1, &[], &[])));
    }

    #[test]
    fn quest_not_finished() {
        assert!(evaluate("Qf!=42", &ctx(1, 1, &[], &[])));
        assert!(!evaluate("Qf!=42", &ctx(1, 1, &[42], &[])));
    }

    #[test]
    fn quest_active() {
        assert!(evaluate("Qa=10", &ctx(1, 1, &[], &[10])));
        assert!(!evaluate("Qa=10", &ctx(1, 1, &[], &[])));
    }

    #[test]
    fn and_operator() {
        assert!(evaluate("PL>5&Qf=42", &ctx(10, 1, &[42], &[])));
        assert!(!evaluate("PL>5&Qf=42", &ctx(10, 1, &[], &[])));
        assert!(!evaluate("PL>5&Qf=42", &ctx(3, 1, &[42], &[])));
    }

    #[test]
    fn or_operator() {
        assert!(evaluate("PL>50|Qf=42", &ctx(10, 1, &[42], &[])));
        assert!(evaluate("PL>50|Qf=42", &ctx(100, 1, &[], &[])));
        assert!(!evaluate("PL>50|Qf=42", &ctx(10, 1, &[], &[])));
    }

    #[test]
    fn parentheses() {
        // (level > 5 AND breed=8) OR quest 42 done
        assert!(evaluate("(PL>5&Ps=8)|Qf=42", &ctx(10, 8, &[], &[])));
        assert!(evaluate("(PL>5&Ps=8)|Qf=42", &ctx(1, 1, &[42], &[])));
        assert!(!evaluate("(PL>5&Ps=8)|Qf=42", &ctx(10, 3, &[], &[])));
    }

    #[test]
    fn complex_criterion() {
        // Real Dofus example: level >= 1 AND quest 489 finished
        assert!(evaluate("PL>=1&Qf=489", &ctx(10, 1, &[489], &[])));
        assert!(!evaluate("PL>=1&Qf=489", &ctx(10, 1, &[], &[])));
    }

    #[test]
    fn unknown_criterion_passes() {
        assert!(evaluate("XX=5", &ctx(1, 1, &[], &[])));
    }

    #[test]
    fn subscribed_always_true() {
        assert!(evaluate("Pb=1", &ctx(1, 1, &[], &[])));
    }
}
