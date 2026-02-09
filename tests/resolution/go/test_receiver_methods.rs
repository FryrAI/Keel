// Tests for Go receiver method resolution (Spec 004 - Go Resolution)
//
// use keel_parsers::go::GoHeuristicResolver;

#[test]
#[ignore = "Not yet implemented"]
/// Pointer receiver methods should resolve when called on a pointer.
fn test_pointer_receiver_method() {
    // GIVEN `func (s *Service) Process()` and a call `svc.Process()` where svc is *Service
    // WHEN the call is resolved
    // THEN it resolves to the pointer receiver method on Service
}

#[test]
#[ignore = "Not yet implemented"]
/// Value receiver methods should resolve when called on a value.
fn test_value_receiver_method() {
    // GIVEN `func (s Service) String() string` and a call `svc.String()`
    // WHEN the call is resolved
    // THEN it resolves to the value receiver method on Service
}

#[test]
#[ignore = "Not yet implemented"]
/// Value receiver methods should also be callable on pointers (Go auto-deref).
fn test_value_receiver_on_pointer() {
    // GIVEN `func (s Service) String() string` and a call `(&svc).String()`
    // WHEN the call is resolved
    // THEN it resolves to the value receiver method (Go auto-dereferences)
}

#[test]
#[ignore = "Not yet implemented"]
/// Embedded struct methods should be promoted and resolvable on the outer struct.
fn test_embedded_struct_method_promotion() {
    // GIVEN struct Outer embedding Inner, Inner has method Do()
    // WHEN outer.Do() is called
    // THEN it resolves to Inner.Do() via method promotion
}

#[test]
#[ignore = "Not yet implemented"]
/// Method name collisions between embedded struct and outer struct should resolve to outer.
fn test_method_name_collision_outer_wins() {
    // GIVEN struct Outer embedding Inner, both define Process()
    // WHEN outer.Process() is called
    // THEN it resolves to Outer.Process() (outer method shadows embedded)
}
