use sails_rs::{
    calls::*,
    gtest::{calls::*, System},
    prelude::*,
};

use manager_app::PointResult;
use manager_client::traits::*;
use mandelbrot_checker_client::traits::*;
const ACTOR_ID: u64 = 42;

#[tokio::test]
async fn check_points_set() {
    let system = System::new();
    system.init_logger();
    system.mint_to(ACTOR_ID, 100_000_000_000_000_000);

    let remoting = GTestRemoting::new(system, ACTOR_ID.into());
    remoting.system().init_logger();

    // Submit program code into the system
    let program_code_id = remoting.system().submit_code(manager::WASM_BINARY);
    let checker_code_id = remoting
        .system()
        .submit_code(mandelbrot_checker::WASM_BINARY);

    let checker_factory =
        mandelbrot_checker_client::MandelbrotCheckerFactory::new(remoting.clone());

    let mut checkers: Vec<ActorId> = Vec::new();

    for i in 0..16 {
        let program_id = checker_factory
            .new()
            .send_recv(checker_code_id, &[i])
            .await
            .unwrap();
        checkers.push(program_id.into());
    }
    let program_factory = manager_client::ManagerFactory::new(remoting.clone());

    let program_id = program_factory
        .new()
        .send_recv(program_code_id, b"salt")
        .await
        .unwrap();

    let mut service_client = manager_client::Manager::new(remoting.clone());
    let checker_bytes: Vec<[u16; 32]> = checkers
        .iter()
        .map(|actor_id| {
            let bytes: [u8; 32] = actor_id.into_bytes();

            let mut arr = [0u16; 32];
            for (i, b) in bytes.iter().enumerate() {
                arr[i] = *b as u16;
            }
            arr
        })
        .collect();

    service_client
        .add_checkers(checker_bytes)
        .send_recv(program_id)
        .await
        .unwrap();

    let checkers_bytes = service_client
        .get_checkers()
        .recv(program_id)
        .await
        .unwrap();

    let checkers =
        Vec::<ActorId>::decode(&mut checkers_bytes.as_slice()).expect("Unable to decode");
    assert_eq!(checkers.len(), 16);

    let width = 200;
    let height = 200;

    let x_min_num = -2;
    let x_min_scale = 0;
    let x_max_num = 1;
    let x_max_scale = 0;
    let y_min_num = -15;
    let y_min_scale = 1;
    let y_max_num = 15;
    let y_max_scale = 1;
    for _i in 0..12 {
        service_client
            .generate_and_store_points(
                width,
                height,
                x_min_num,
                x_min_scale,
                x_max_num,
                x_max_scale,
                y_min_num,
                y_min_scale,
                y_max_num,
                y_max_scale,
                30_000,
                false,
                0,
                0,
                0,
            )
            .send_recv(program_id)
            .await
            .unwrap();
    }

    let points_len = service_client
        .get_points_len()
        .recv(program_id)
        .await
        .unwrap();
    assert_eq!(points_len, 40_000);
    //assert_eq!(points_len, 360_000);

    for _i in 0..40 {
        println!("{:?}", _i);
        service_client
            .check_points_set(1000, 80, 1)
            .send_recv(program_id)
            .await
            .unwrap();
        remoting.system().run_next_block();
    }

    let msg_sent = service_client.points_sent().recv(program_id).await.unwrap();

    assert_eq!(msg_sent, 40_000);

    let point_results_bytes = service_client
        .get_results(0, 40_000)
        .recv(program_id)
        .await
        .unwrap();
    let point_results =
        Vec::<PointResult>::decode(&mut point_results_bytes.as_slice()).expect("Unable to decode");
    println!("Number of points {:?}", point_results.len());
    if point_results.iter().all(|point| point.checked) {
        println!("All points are checked!");
    } else {
        let unchecked_count = point_results.iter().filter(|point| !point.checked).count();
        println!(
            "Some points are not checked. Unchecked points: {}",
            unchecked_count
        );
    }
}
