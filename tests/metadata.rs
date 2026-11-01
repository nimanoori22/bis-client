use bis_client::StructureRef;

#[test]
fn urn_references_are_validated_before_use() {
    let id = StructureRef::from_urn(
        "urn:sdmx:org.sdmx.infomodel.datastructure.DataStructure=BIS:BIS_EER(1.0)",
    )
    .unwrap();
    assert_eq!(id, StructureRef::new("BIS", "BIS_EER", "1.0"));
    for urn in [
        "bad",
        "BIS:BIS_EER(1.0)",
        "urn:sdmx:org.sdmx.infomodel.x=BIS:../x(1.0)",
        "urn:sdmx:org.sdmx.infomodel.x=BIS:CL_FREQ(1.0).M",
        "urn:sdmx:org.sdmx.infomodel.x=BIS:CL_FREQ()",
    ] {
        assert!(StructureRef::from_urn(urn).is_err(), "{urn}");
    }
}
