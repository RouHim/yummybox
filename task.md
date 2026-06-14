Lets build a simple meal manager web application.
Lets start with a mvp:
* no user management / user
* A view vor displaying / managing all meals
* A way to add new meals
* Lets keep it simple (KISS)
* design simple / minimal / no ai slop
... for the rest lets run a product brainstorm 

## Tech Stack

- backend Programming Language: Rust 
- frontend svetle
- sqlite Database

### Technical Requirements

- External crates are allowed, but keep them as low as possible
- Prefer standard Rust libraries and built-in features to minimize external package usage.
- Evaluate trade-offs before adding any third-party crate.
- When using external crates, make sure to use the very latest stable versions.
- All static files needs to be embedded into the binary
- Must compile and run without errors
- Handle user interactions gracefully
- Implement proper error handling and validation
- Use appropriate Rust idioms and patterns
- Logging: prefer `tracing`/`tracing_subscriber` with contextual spans instead of `println!`.
- Error handling: avoid `unwrap`/`expect` in non-test code; surface actionable errors to the UI.
- Structure code into small, focused rust files without using rust modules
- Each file should encapsulate a single responsibility or closely related functionalities.
- Promote reusability and ease of testing by isolating components.
- Follow the SOLID object-oriented design principles to ensure maintainable and extensible code.
- Emphasize single responsibility, open-closed, Liskov substitution, interface segregation, and dependency inversion
  where applicable.
- Use descriptive names and avoid clever tricks or shortcuts that hinder comprehensibility.
- YAGNI - You Aren't Gonna Need It: Avoid adding functionality until it is necessary.
- Don't write unused code for future features.
- Always run code formatters (`cargo fmt`) and linters (`cargo clippy`) when finishing a task.
- Maintain consistent code style across the project to improve readability and reduce friction in reviews.
- Always use RustTLS for any TLS connections, no OpenSSL.
- When planning a new feature, do not only rely on your training data, also get a second opinion via the web search.

## Testing Practices

### Test-Driven Development (TDD)

- Prefer write tests before writing the functionality.
- Use tests to drive design decisions and ensure robust feature implementation.

### Behavior-Driven Development (BDD)

- Write tests in a BDD style, focusing on the expected behavior and outcomes.
- Structure tests to clearly state scenarios, actions, and expected results to improve communication and documentation.

