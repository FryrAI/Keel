// Tests for TypeScript decorator resolution (Spec 002 - TypeScript Resolution)
//
// use keel_parsers::typescript::OxcResolver;

#[test]
#[ignore = "Not yet implemented"]
/// Class decorators should be resolved to their definition.
fn test_class_decorator_resolution() {
    // GIVEN a TypeScript class with `@Injectable()` decorator
    // WHEN the decorator reference is resolved
    // THEN it links to the Injectable decorator function definition
}

#[test]
#[ignore = "Not yet implemented"]
/// Method decorators should create edges to the decorator function.
fn test_method_decorator_resolution() {
    // GIVEN a class method with `@Log` decorator
    // WHEN the decorator is resolved
    // THEN a Calls edge from the method to the Log decorator is created
}

#[test]
#[ignore = "Not yet implemented"]
/// Parameter decorators should be tracked for DI framework analysis.
fn test_parameter_decorator_resolution() {
    // GIVEN a constructor with `@Inject(TOKEN)` parameter decorator
    // WHEN the decorator is resolved
    // THEN the injection dependency is tracked as an edge
}

#[test]
#[ignore = "Not yet implemented"]
/// Decorators imported from external packages should have lower confidence.
fn test_external_decorator_lower_confidence() {
    // GIVEN a decorator imported from a node_modules package
    // WHEN the decorator reference is resolved
    // THEN the edge has lower confidence than locally-defined decorators
}

#[test]
#[ignore = "Not yet implemented"]
/// Multiple decorators on a single class should all be resolved.
fn test_multiple_decorators_on_class() {
    // GIVEN a class with @Controller('/api') and @Authenticated decorators
    // WHEN both decorators are resolved
    // THEN edges to both decorator definitions are created
}

#[test]
#[ignore = "Not yet implemented"]
/// Decorator factories (decorators returning decorators) should resolve to the factory.
fn test_decorator_factory_resolution() {
    // GIVEN `@Throttle(100)` where Throttle is a factory returning a decorator
    // WHEN the decorator is resolved
    // THEN it resolves to the Throttle factory function
}
