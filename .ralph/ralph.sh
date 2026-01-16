#!/bin/bash

# Ralph Wiggum Technique - Continuous Agent Loop
# This script runs an AI coding agent in a loop until all features are complete

set -e

# Configuration
MAX_ITERATIONS=${MAX_ITERATIONS:-100}
SLEEP_BETWEEN_ITERATIONS=${SLEEP_BETWEEN_ITERATIONS:-5}
AGENT_PROMPT_FILE="${AGENT_PROMPT_FILE:-AGENT_PROMPT.md}"
PRD_FILE="${PRD_FILE:-.ralph/prd.json}"
PROGRESS_FILE="${PROGRESS_FILE:-.ralph/progress.txt}"

# Git Safety Configuration
# Protect important branches from direct commits
PROTECTED_BRANCHES="${PROTECTED_BRANCHES:-main,master}"
# Allow git push operations (default: false for safety)
ALLOW_GIT_PUSH="${ALLOW_GIT_PUSH:-false}"
# Auto-create feature branches when on protected branch (default: true)
AUTO_CREATE_BRANCH="${AUTO_CREATE_BRANCH:-true}"

# Run Mode Configuration
# Set how the script executes iterations:
# Options:
#   "once"       - Human-in-the-loop: Run one iteration, then stop for review (default)
#   "continuous" - AFK mode: Run until all features complete or max iterations reached
RUN_MODE="${RUN_MODE:-once}"

# AI Agent Configuration
# Set your preferred AI agent command here
# Options:
#   "manual"  - Prompts you to run the agent manually (default for compatibility)
#   "claude"  - Uses Claude CLI (requires: npm install -g @anthropic-ai/claude-cli)
#   "cursor"  - Uses Cursor CLI (if available)
#   "custom"  - Set AI_AGENT_CUSTOM_CMD below for your own command
AI_AGENT_MODE="${AI_AGENT_MODE:-claude}"

# For custom commands, set this variable:
# Example: AI_AGENT_CUSTOM_CMD="your-ai-tool --prompt-file"
AI_AGENT_CUSTOM_CMD="${AI_AGENT_CUSTOM_CMD:-}"

# Git Safety Configuration
# Prevent commits to these branches (comma-separated)
PROTECTED_BRANCHES="${PROTECTED_BRANCHES:-main,master}"

# Allow git push to remote (default: false for safety)
ALLOW_GIT_PUSH="${ALLOW_GIT_PUSH:-false}"

# Error Recovery Configuration
# Automatically rollback git commit if tests fail after implementation
ROLLBACK_ON_FAILURE="${ROLLBACK_ON_FAILURE:-true}"

# Run verification tests before accepting a feature as complete
VERIFY_BEFORE_COMPLETE="${VERIFY_BEFORE_COMPLETE:-true}"

# Code Quality Configuration
# Automatically fix prettier formatting issues before verification
AUTOFIX_PRETTIER="${AUTOFIX_PRETTIER:-true}"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Function to print colored output
log_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

log_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

log_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Check if current branch is protected
is_protected_branch() {
    local current_branch=$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo "")

    if [ -z "$current_branch" ]; then
        return 1
    fi

    # Convert comma-separated list to array
    IFS=',' read -ra PROTECTED <<< "$PROTECTED_BRANCHES"

    for protected in "${PROTECTED[@]}"; do
        # Trim whitespace
        protected=$(echo "$protected" | xargs)
        if [ "$current_branch" = "$protected" ]; then
            return 0
        fi
    done

    return 1
}

# Suggest creating a feature branch
suggest_feature_branch() {
    local current_branch=$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo "unknown")

    echo ""
    log_error "Cannot run Ralph on protected branch: $current_branch"
    echo ""
    log_info "Ralph prevents commits to protected branches for safety."
    log_info "Protected branches: $PROTECTED_BRANCHES"
    echo ""
    log_info "To continue, create a feature branch:"
    echo "  git checkout -b feature/your-feature-name"
    echo ""
    log_info "Or override protection (not recommended):"
    echo "  PROTECTED_BRANCHES=\"\" $0"
    echo ""
}

# Get next feature from PRD (highest priority incomplete feature with met dependencies)
get_next_feature_from_prd() {
    if [ ! -f "$PRD_FILE" ]; then
        echo ""
        return 1
    fi

    # Use Python to parse JSON and find next feature
    python3 -c "
import json
import sys

try:
    with open('$PRD_FILE', 'r') as f:
        prd = json.load(f)

    features = prd.get('features', [])

    # Priority order
    priority_order = {'critical': 0, 'high': 1, 'medium': 2, 'low': 3}

    # Filter to incomplete features with met dependencies
    candidates = []
    for feature in features:
        if feature.get('passes', False):
            continue
        if feature.get('blocked_reason'):
            continue

        # Check dependencies
        depends_on = feature.get('depends_on', [])
        deps_met = True
        for dep_id in depends_on:
            dep_feature = next((f for f in features if f.get('id') == dep_id), None)
            if not dep_feature or not dep_feature.get('passes', False):
                deps_met = False
                break

        if deps_met:
            candidates.append(feature)

    if not candidates:
        sys.exit(1)

    # Sort by priority
    candidates.sort(key=lambda f: (
        priority_order.get(f.get('priority', 'low'), 99),
        f.get('id', '')
    ))

    # Output first candidate as JSON
    print(json.dumps(candidates[0]))
except Exception as e:
    sys.stderr.write(f'Error parsing PRD: {e}\n')
    sys.exit(1)
" 2>/dev/null
}

# Generate branch name from feature
generate_branch_name() {
    local feature_json="$1"

    if [ -z "$feature_json" ]; then
        # Fallback to generic name with timestamp
        echo "feature/ralph-$(date +%Y%m%d-%H%M%S)"
        return
    fi

    # Parse feature JSON
    local feature_id=$(echo "$feature_json" | python3 -c "import json,sys; print(json.load(sys.stdin).get('id', ''))")
    local feature_type=$(echo "$feature_json" | python3 -c "import json,sys; print(json.load(sys.stdin).get('type', 'feature'))")
    local feature_desc=$(echo "$feature_json" | python3 -c "import json,sys; print(json.load(sys.stdin).get('description', ''))")

    # Determine branch prefix based on type
    local prefix="feature"
    case "$feature_type" in
        bug) prefix="bugfix" ;;
        refactor) prefix="refactor" ;;
        test) prefix="test" ;;
        feature|*) prefix="feature" ;;
    esac

    # Create slug from description (first few words, lowercase, dashes)
    local slug=$(echo "$feature_desc" | tr '[:upper:]' '[:lower:]' | sed 's/[^a-z0-9 ]//g' | tr -s ' ' | cut -d' ' -f1-4 | tr ' ' '-')

    # Construct branch name
    if [ -n "$feature_id" ] && [ -n "$slug" ]; then
        echo "${prefix}/${feature_id}-${slug}"
    elif [ -n "$feature_id" ]; then
        echo "${prefix}/${feature_id}"
    else
        echo "${prefix}/ralph-$(date +%Y%m%d-%H%M%S)"
    fi
}

# Auto-create and checkout feature branch
auto_create_feature_branch() {
    local branch_name="$1"

    if [ -z "$branch_name" ]; then
        log_error "No branch name provided"
        return 1
    fi

    log_info "Auto-creating feature branch: $branch_name"

    if git show-ref --verify --quiet "refs/heads/$branch_name"; then
        log_warning "Branch $branch_name already exists, checking out..."
        git checkout "$branch_name"
    else
        log_info "Creating new branch from current HEAD..."
        git checkout -b "$branch_name"
    fi

    log_success "Now on branch: $branch_name"
    return 0
}

# Check prerequisites
check_prerequisites() {
    log_info "Checking prerequisites..."

    # Check if on protected branch
    if is_protected_branch; then
        if [ "$AUTO_CREATE_BRANCH" = "true" ]; then
            log_warning "Currently on protected branch: $(git rev-parse --abbrev-ref HEAD)"
            echo ""

            # Check if user provided custom branch name
            if [ -n "$CUSTOM_BRANCH_NAME" ]; then
                log_info "Using custom branch name: $CUSTOM_BRANCH_NAME"
                auto_create_feature_branch "$CUSTOM_BRANCH_NAME" || {
                    suggest_feature_branch
                    exit 1
                }
            else
                # Auto-detect next feature and create branch
                log_info "AUTO_CREATE_BRANCH is enabled, inspecting PRD for next feature..."
                local next_feature=$(get_next_feature_from_prd)

                if [ -n "$next_feature" ]; then
                    local feature_desc=$(echo "$next_feature" | python3 -c "import json,sys; print(json.load(sys.stdin).get('description', ''))")
                    log_info "Next feature: $feature_desc"

                    local branch_name=$(generate_branch_name "$next_feature")
                    log_info "Generated branch name: $branch_name"
                    echo ""

                    auto_create_feature_branch "$branch_name" || {
                        suggest_feature_branch
                        exit 1
                    }
                else
                    log_warning "Could not determine next feature from PRD"
                    log_info "Creating generic feature branch..."
                    local fallback_branch="feature/ralph-$(date +%Y%m%d-%H%M%S)"
                    auto_create_feature_branch "$fallback_branch" || {
                        suggest_feature_branch
                        exit 1
                    }
                fi
            fi

            echo ""
            log_success "Branch created successfully! Ready to run Ralph."
            echo ""
        else
            suggest_feature_branch
            exit 1
        fi
    fi

    if [ ! -f "$AGENT_PROMPT_FILE" ]; then
        log_error "Agent prompt file not found: $AGENT_PROMPT_FILE"
        exit 1
    fi

    # Check if .ralph directory exists
    if [ ! -d ".ralph" ]; then
        log_error ".ralph directory not found. Have you initialized the project?"
        log_info "Run the initializer agent first, or create .ralph/ directory manually"
        exit 1
    fi

    if [ ! -f "$PRD_FILE" ]; then
        log_error "PRD file not found: $PRD_FILE"
        exit 1
    fi

    if [ ! -f "$PROGRESS_FILE" ]; then
        log_warning "Progress file not found. Creating $PROGRESS_FILE"
        touch "$PROGRESS_FILE"
        echo "=== Ralph Wiggum Progress Log ===" > "$PROGRESS_FILE"
        echo "Started: $(date)" >> "$PROGRESS_FILE"
        echo "" >> "$PROGRESS_FILE"
    fi

    # Check if git repo exists
    if [ ! -d ".git" ]; then
        log_warning "No git repository found. Initializing..."
        git init
        git add .
        git commit -m "Initial commit - Ralph Wiggum setup"
    fi

    log_success "Prerequisites check complete"
}

# Check if all features are complete
check_completion() {
    local incomplete_count=$(grep -c '"passes": false' "$PRD_FILE" || true)

    if [ "$incomplete_count" -eq 0 ]; then
        log_success "All features complete!"
        echo "  🎓 \"I'm learnding!\" - Ralph Wiggum"
        echo ""
        return 0
    else
        log_info "Remaining features: $incomplete_count"
        return 1
    fi
}

# Run verification tests
run_verification_tests() {
    log_info "Running code quality gates..."
    echo ""

    local tests_passed=true
    local quality_gate_results=""

    # Gate 1: Code Formatting (Prettier/Black/etc)
    if [ -f "package.json" ]; then
        if grep -q '"format:check"' package.json || grep -q '"prettier"' package.json; then
            log_info "🎨 Quality Gate 1/4: Code Formatting"

            # Auto-fix if enabled
            if [ "$AUTOFIX_PRETTIER" = "true" ]; then
                if grep -q '"format"' package.json; then
                    log_info "Auto-fixing formatting issues..."
                    npm run format 2>&1 | tee /tmp/ralph_format.log || true
                fi
            fi

            # Check formatting
            if grep -q '"format:check"' package.json; then
                if ! npm run format:check 2>&1 | tee /tmp/ralph_format_check.log; then
                    log_error "❌ FAILED: Code formatting issues detected"
                    log_info "Fix with: npm run format"
                    quality_gate_results="${quality_gate_results}\n  ❌ Formatting"
                    tests_passed=false
                else
                    log_success "✅ PASSED: Code formatting"
                    quality_gate_results="${quality_gate_results}\n  ✅ Formatting"
                fi
            elif command -v prettier &> /dev/null; then
                if ! prettier --check . 2>&1 | tee /tmp/ralph_prettier.log; then
                    log_error "❌ FAILED: Prettier formatting issues detected"
                    log_info "Fix with: prettier --write ."
                    quality_gate_results="${quality_gate_results}\n  ❌ Formatting"
                    tests_passed=false
                else
                    log_success "✅ PASSED: Prettier formatting"
                    quality_gate_results="${quality_gate_results}\n  ✅ Formatting"
                fi
            else
                log_info "⊘ SKIPPED: No formatting tool configured"
                quality_gate_results="${quality_gate_results}\n  ⊘ Formatting (not configured)"
            fi
            echo ""
        fi
    fi

    # Gate 2: Linting (MUST PASS - blocking)
    log_info "🔍 Quality Gate 2/4: Linting"
    if [ -f "package.json" ] && grep -q '"lint"' package.json; then
        if ! npm run lint 2>&1 | tee /tmp/ralph_lint.log; then
            log_error "❌ FAILED: Linting errors detected (BLOCKING)"
            log_info "Fix linting issues before marking feature complete"
            quality_gate_results="${quality_gate_results}\n  ❌ Linting"
            tests_passed=false
        else
            log_success "✅ PASSED: Linting"
            quality_gate_results="${quality_gate_results}\n  ✅ Linting"
        fi
    else
        log_info "⊘ SKIPPED: No linting configured"
        quality_gate_results="${quality_gate_results}\n  ⊘ Linting (not configured)"
    fi
    echo ""

    # Gate 3: Type Checking (MUST PASS if configured)
    log_info "🔎 Quality Gate 3/4: Type Checking"
    if [ -f "package.json" ] && grep -q '"typecheck"' package.json; then
        if ! npm run typecheck 2>&1 | tee /tmp/ralph_typecheck.log; then
            log_error "❌ FAILED: Type checking errors detected (BLOCKING)"
            log_info "Fix type errors before marking feature complete"
            quality_gate_results="${quality_gate_results}\n  ❌ Type Checking"
            tests_passed=false
        else
            log_success "✅ PASSED: Type checking"
            quality_gate_results="${quality_gate_results}\n  ✅ Type Checking"
        fi
    elif [ -f "tsconfig.json" ] || [ -f "package.json" ] && grep -q '"typescript"' package.json; then
        if command -v tsc &> /dev/null; then
            if ! tsc --noEmit 2>&1 | tee /tmp/ralph_tsc.log; then
                log_error "❌ FAILED: TypeScript compilation errors (BLOCKING)"
                quality_gate_results="${quality_gate_results}\n  ❌ Type Checking"
                tests_passed=false
            else
                log_success "✅ PASSED: TypeScript"
                quality_gate_results="${quality_gate_results}\n  ✅ Type Checking"
            fi
        else
            log_info "⊘ SKIPPED: TypeScript found but tsc not available"
            quality_gate_results="${quality_gate_results}\n  ⊘ Type Checking (tsc not found)"
        fi
    else
        log_info "⊘ SKIPPED: No type checking configured"
        quality_gate_results="${quality_gate_results}\n  ⊘ Type Checking (not configured)"
    fi
    echo ""

    # Gate 4: Test Suite (MUST PASS if tests exist)
    log_info "🧪 Quality Gate 4/4: Test Suite"
    if [ -f "package.json" ] && grep -q '"test"' package.json; then
        if ! npm test 2>&1 | tee /tmp/ralph_test.log; then
            log_error "❌ FAILED: Test suite failed (BLOCKING)"
            log_info "Fix failing tests before marking feature complete"
            quality_gate_results="${quality_gate_results}\n  ❌ Tests"
            tests_passed=false
        else
            log_success "✅ PASSED: Test suite"
            quality_gate_results="${quality_gate_results}\n  ✅ Tests"
        fi
    else
        log_info "⊘ SKIPPED: No tests configured"
        quality_gate_results="${quality_gate_results}\n  ⊘ Tests (not configured)"
    fi
    echo ""

    # Summary
    echo "=========================================="
    log_info "Quality Gate Summary:"
    echo -e "$quality_gate_results"
    echo "=========================================="
    echo ""

    if [ "$tests_passed" = true ]; then
        log_success "✅ ALL QUALITY GATES PASSED"
        return 0
    else
        log_error "❌ QUALITY GATES FAILED - Feature cannot be marked complete"
        echo "  💭 \"Me fail English? That's unpossible!\" - Ralph Wiggum"
        log_warning "Fix the issues above and try again"
        return 1
    fi
}

# Rollback the last commit
rollback_last_commit() {
    log_warning "Rolling back last commit... 🧸"
    echo "  💭 \"I bent my Wookiee!\" - Ralph Wiggum"
    echo ""

    local last_commit=$(git log -1 --pretty=%B 2>/dev/null || echo "")
    if [ -n "$last_commit" ]; then
        log_info "Reverting: $last_commit"
        git reset --hard HEAD~1
        log_success "Rollback complete"
    else
        log_warning "No commit to rollback"
    fi
}

# Check for unauthorized git push operations
check_for_git_push() {
    if [ "$ALLOW_GIT_PUSH" != "true" ]; then
        # Check if last git command was a push
        local git_reflog=$(git reflog -1 2>/dev/null || echo "")
        if echo "$git_reflog" | grep -q "push"; then
            log_error "Git push detected but ALLOW_GIT_PUSH is not enabled!"
            log_warning "For safety, Ralph blocks git push by default."
            log_info "To enable pushing:"
            echo "  ALLOW_GIT_PUSH=true $0"
            echo ""
            return 1
        fi
    fi
    return 0
}

# Main loop
run_ralph_loop() {
    if [ "$RUN_MODE" = "once" ]; then
        log_info "Starting Ralph Wiggum (human-in-the-loop mode - single iteration)"
        echo "  🍌 \"Go banana!\" - Ralph Wiggum"
        echo ""
        run_single_iteration 1
    else
        log_info "Starting Ralph Wiggum loop (continuous mode - max $MAX_ITERATIONS iterations)"
        echo "  🍌 \"Go banana!\" - Ralph Wiggum"
        echo ""
        run_continuous_loop
    fi
}

# Run a single iteration (human-in-the-loop)
run_single_iteration() {
    local iteration=${1:-1}

    echo ""
    echo "=========================================="
    log_info "Iteration $iteration (Human-in-the-Loop)"
    echo "=========================================="
    echo ""

    # Check if already complete
    if check_completion; then
        log_success "Project complete! Exiting."
        exit 0
    fi

    # Run the agent based on configured mode
    execute_agent

    # Check if agent declared completion
    if check_completion; then
        log_success "PROMISE COMPLETE - All features implemented!"
        exit 0
    fi

    # Verify a commit was made
    LAST_COMMIT_MESSAGE=$(git log -1 --pretty=%B 2>/dev/null || echo "")
    if [ -n "$LAST_COMMIT_MESSAGE" ]; then
        log_success "Commit detected: $LAST_COMMIT_MESSAGE"

        # Check for unauthorized git push
        check_for_git_push

        # Run verification tests if enabled
        if [ "$VERIFY_BEFORE_COMPLETE" = "true" ]; then
            if ! run_verification_tests; then
                if [ "$ROLLBACK_ON_FAILURE" = "true" ]; then
                    log_error "Verification failed - rolling back changes"
                    rollback_last_commit
                    echo ""
                    log_warning "Feature may need to be reworked or marked as blocked"
                else
                    log_warning "Verification failed but rollback is disabled"
                    log_info "You may want to fix issues before continuing"
                fi
            else
                log_success "Verification passed - changes accepted"
            fi
        fi
    else
        log_warning "No git commit detected in this iteration"
    fi

    echo ""
    echo "=========================================="
    log_success "Iteration Complete"
    echo "=========================================="
    echo ""
    log_info "Review the changes made:"
    echo "  - Check git diff: git diff HEAD~1"
    echo "  - Review PRD: cat $PRD_FILE"
    echo "  - Check progress: tail -20 $PROGRESS_FILE"
    echo ""
    log_info "To continue with the next iteration, run: $0"
    echo ""
}

# Run continuous loop (AFK mode)
run_continuous_loop() {
    for iteration in $(seq 1 $MAX_ITERATIONS); do
        echo ""
        echo "=========================================="
        log_info "Iteration $iteration of $MAX_ITERATIONS (Continuous Mode)"
        echo "=========================================="
        echo ""

        # Check if already complete
        if check_completion; then
            log_success "Project complete! Exiting."
            exit 0
        fi

        # Run the agent
        execute_agent

        # Check if agent declared completion
        if check_completion; then
            log_success "PROMISE COMPLETE - All features implemented!"
            exit 0
        fi

        # Verify a commit was made
        LAST_COMMIT_MESSAGE=$(git log -1 --pretty=%B 2>/dev/null || echo "")
        if [ -n "$LAST_COMMIT_MESSAGE" ]; then
            log_success "Commit detected: $LAST_COMMIT_MESSAGE"

            # Check for unauthorized git push
            check_for_git_push

            # Run verification tests if enabled
            if [ "$VERIFY_BEFORE_COMPLETE" = "true" ]; then
                if ! run_verification_tests; then
                    if [ "$ROLLBACK_ON_FAILURE" = "true" ]; then
                        log_error "Verification failed - rolling back changes"
                        rollback_last_commit
                        log_warning "Continuing to next iteration (feature may be blocked)"
                    else
                        log_warning "Verification failed but rollback is disabled"
                    fi
                else
                    log_success "Verification passed - changes accepted"
                fi
            fi
        else
            log_warning "No git commit detected in this iteration"
        fi

        # Sleep before next iteration (give user time to review)
        if [ $iteration -lt $MAX_ITERATIONS ]; then
            log_info "Waiting $SLEEP_BETWEEN_ITERATIONS seconds before next iteration..."
            sleep $SLEEP_BETWEEN_ITERATIONS
        fi
    done

    echo ""
    log_warning "Reached maximum iterations ($MAX_ITERATIONS) without completing all features"
    check_completion
}

# Execute the AI agent based on configured mode
execute_agent() {
    log_info "Running agent (mode: $AI_AGENT_MODE)..."

    case "$AI_AGENT_MODE" in
        claude)
            log_info "Using Claude CLI..."
            if command -v claude &> /dev/null; then
                cat "$AGENT_PROMPT_FILE" | claude
            elif command -v npx &> /dev/null; then
                log_info "Claude CLI not found globally, trying npx..."
                cat "$AGENT_PROMPT_FILE" | npx -y @anthropic-ai/claude-cli
            else
                log_error "Claude CLI not found. Install with: npm install -g @anthropic-ai/claude-cli"
                log_info "Falling back to manual mode for this iteration..."
                AI_AGENT_MODE="manual"
                execute_agent
            fi
            ;;
        cursor)
            log_info "Using Cursor CLI..."
            if command -v cursor-agent &> /dev/null; then
                cat "$AGENT_PROMPT_FILE" | cursor-agent
            else
                log_error "Cursor CLI not found"
                log_info "Falling back to manual mode for this iteration..."
                AI_AGENT_MODE="manual"
                execute_agent
            fi
            ;;
        custom)
            if [ -n "$AI_AGENT_CUSTOM_CMD" ]; then
                log_info "Using custom command: $AI_AGENT_CUSTOM_CMD"
                cat "$AGENT_PROMPT_FILE" | $AI_AGENT_CUSTOM_CMD
            else
                log_error "AI_AGENT_CUSTOM_CMD not set"
                log_info "Falling back to manual mode for this iteration..."
                AI_AGENT_MODE="manual"
                execute_agent
            fi
            ;;
        manual|*)
            echo ""
            log_warning "MANUAL STEP REQUIRED:"
            echo "1. Open your AI agent (Claude, Cursor, etc.)"
            echo "2. Provide this file: $AGENT_PROMPT_FILE"
            echo "3. Let the agent complete its work"
            echo "4. Check the terminal for git commit"
            echo ""
            read -p "Press Enter when agent has completed this iteration..."
            ;;
    esac
}

# Main execution
main() {
    # Parse command line arguments
    CUSTOM_BRANCH_NAME=""
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --branch-name)
                CUSTOM_BRANCH_NAME="$2"
                shift 2
                ;;
            --help|-h)
                echo "Ralph Wiggum Technique - Autonomous Coding Agent"
                echo ""
                echo "Usage: $0 [OPTIONS]"
                echo ""
                echo "Options:"
                echo "  --branch-name NAME    Specify custom branch name for auto-creation"
                echo "  --help, -h            Show this help message"
                echo ""
                echo "Environment Variables:"
                echo "  RUN_MODE              'once' (default) or 'continuous'"
                echo "  AUTO_CREATE_BRANCH    'true' (default) or 'false'"
                echo "  PROTECTED_BRANCHES    Comma-separated list (default: 'main,master')"
                echo "  ALLOW_GIT_PUSH        'true' or 'false' (default: false)"
                echo ""
                exit 0
                ;;
            *)
                log_error "Unknown option: $1"
                echo "Use --help for usage information"
                exit 1
                ;;
        esac
    done

    echo ""
    echo "╔════════════════════════════════════════╗"
    echo "║   Ralph Wiggum Technique - Agent Loop  ║"
    if [ "$RUN_MODE" = "once" ]; then
        echo "║        Human-in-the-Loop Mode          ║"
    else
        echo "║          Continuous AFK Mode           ║"
    fi
    echo "╔════════════════════════════════════════╗"
    echo ""

    check_prerequisites
    run_ralph_loop
}

# Run main function
main "$@"
