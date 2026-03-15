use reaper_test::reaper_test;

#[reaper_test()]
async fn reaper_test(ctx: &ReaperTestContext) -> Result<()> {
    let project = ctx.project();

    project.add_track("testing", None).await.unwrap();
    let tracks = project.tracks().all().await.unwrap();

    Ok(())
}
