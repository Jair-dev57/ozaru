use ozaru_engine::syntax::{IndirectObject, Name, Object, ObjectId};

#[test]
fn indirect_objects_pair_a_number_with_an_object() {
    let indirect = IndirectObject::new(ObjectId::new(4, 0), Object::Name(Name::from("Page")));

    assert_eq!(indirect.id, ObjectId::new(4, 0));
    assert_eq!(indirect.object, Object::Name(Name::from("Page")));
}

#[test]
fn indirect_objects_with_different_generations_are_different() {
    assert_ne!(
        IndirectObject::new(ObjectId::new(4, 0), Object::Null),
        IndirectObject::new(ObjectId::new(4, 1), Object::Null)
    );
}
