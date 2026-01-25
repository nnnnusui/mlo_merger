# Copilot Instructions (Meta-Guidelines)

## Project Management Philosophy

This document contains meta-guidelines that cannot be expressed through automated formatters or linters. For code style rules that can be enforced automatically (indentation, line length, etc.), use `rustfmt.toml` instead.

## Core Directives

### Language & Framework
- **Primary Language**: Rust (stable)
- **Target Platforms**: Windows and Linux
- **Development Environment**: Ubuntu via VSCode .devcontainer

### Documentation Requirements

1. **Code Documentation**
   - All public APIs MUST have doc comments (`///`)
   - Doc comments SHOULD be in English
   - Include examples in doc comments for non-trivial functions
   - Private functions SHOULD have doc comments for complex logic

2. **Project Documentation**
   - `docs/ARCHITECTURE.md`: Application structure, flow, and design decisions
   - `docs/TODO.md`: Project task tracking and roadmap
   - This file (`.github/copilot-instructions.md`): Meta-guidelines

3. **Documentation Updates**
   - Update `docs/TODO.md` when tasks change or complete
   - Update `docs/ARCHITECTURE.md` when structure or design changes
   - Update this file when new meta-guidelines emerge

## Version Control

- Commit messages in English
- Follow conventional commits format
- Keep commits atomic and focused
