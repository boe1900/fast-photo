use fast_photo_core::db;
use fast_photo_core::dedup;
use fast_photo_core::models::{CreateLibrary, CreateUser};
use image::{DynamicImage, GrayImage, Luma};
use std::path::PathBuf;
use uuid::Uuid;

fn temp_db_url() -> (String, PathBuf) {
    let file_name = format!("fast-photo-test-{}.db", Uuid::new_v4());
    let file = PathBuf::from(&file_name);
    (format!("sqlite:{}", file_name), file)
}

#[tokio::test]
async fn folder_and_photo_access_is_scoped_to_library_ids() {
    let (db_url, db_path) = temp_db_url();
    std::fs::File::create(&db_path).expect("create db file");
    let pool = db::init_pool(&db_url).await.expect("init db");

    let u1 = db::create_user(
        &pool,
        &CreateUser {
            username: "u1".to_string(),
            password: "ignored".to_string(),
            role: Some("user".to_string()),
        },
        "hash1",
    )
    .await
    .expect("create user1");
    let u2 = db::create_user(
        &pool,
        &CreateUser {
            username: "u2".to_string(),
            password: "ignored".to_string(),
            role: Some("user".to_string()),
        },
        "hash2",
    )
    .await
    .expect("create user2");

    let lib1 = db::create_library(
        &pool,
        &CreateLibrary {
            name: "lib1".to_string(),
            path: "/tmp/lib1".to_string(),
        },
        u1.id,
    )
    .await
    .expect("create lib1");
    let lib2 = db::create_library(
        &pool,
        &CreateLibrary {
            name: "lib2".to_string(),
            path: "/tmp/lib2".to_string(),
        },
        u2.id,
    )
    .await
    .expect("create lib2");

    let p1 = db::upsert_photo(
        &pool,
        &db::InsertPhoto {
            file_path: "/tmp/lib1/holiday/a.jpg".to_string(),
            file_name: "a.jpg".to_string(),
            file_size: 1,
            file_hash: Some("h1".to_string()),
            mime_type: "image/jpeg".to_string(),
            width: None,
            height: None,
            taken_at: None,
            latitude: None,
            longitude: None,
            camera_make: None,
            camera_model: None,
            library_id: lib1.id,
        },
    )
    .await
    .expect("insert p1");
    let p2 = db::upsert_photo(
        &pool,
        &db::InsertPhoto {
            file_path: "/tmp/lib2/holiday/b.jpg".to_string(),
            file_name: "b.jpg".to_string(),
            file_size: 1,
            file_hash: Some("h2".to_string()),
            mime_type: "image/jpeg".to_string(),
            width: None,
            height: None,
            taken_at: None,
            latitude: None,
            longitude: None,
            camera_make: None,
            camera_model: None,
            library_id: lib2.id,
        },
    )
    .await
    .expect("insert p2");

    let (photos, total) = db::get_photos_by_folder(&pool, &[lib1.id], "/tmp/lib1/holiday", 0, 50)
        .await
        .expect("query folder");
    assert_eq!(total, 1);
    assert_eq!(photos.len(), 1);
    assert_eq!(photos[0].id, p1);

    assert!(db::photo_in_libraries(&pool, p1, &[lib1.id])
        .await
        .expect("access p1"));
    assert!(!db::photo_in_libraries(&pool, p2, &[lib1.id])
        .await
        .expect("access p2"));

    let filtered = db::filter_photo_ids_in_libraries(&pool, &[p1, p2], &[lib1.id])
        .await
        .expect("filter ids");
    assert_eq!(filtered, vec![p1]);

    drop(pool);
    let _ = std::fs::remove_file(db_path);
}

#[tokio::test]
async fn insert_uploaded_photo_returns_stable_id_on_conflict() {
    let (db_url, db_path) = temp_db_url();
    std::fs::File::create(&db_path).expect("create db file");
    let pool = db::init_pool(&db_url).await.expect("init db");

    let user = db::create_user(
        &pool,
        &CreateUser {
            username: "upload_user".to_string(),
            password: "ignored".to_string(),
            role: Some("user".to_string()),
        },
        "hash",
    )
    .await
    .expect("create user");
    let lib = db::create_library(
        &pool,
        &CreateLibrary {
            name: "upload-lib".to_string(),
            path: "/tmp/upload-lib".to_string(),
        },
        user.id,
    )
    .await
    .expect("create lib");

    let p = "/tmp/upload-lib/Uploads/sample.jpg";
    let id1 = db::insert_uploaded_photo(
        &pool,
        p,
        "sample.jpg",
        100,
        "image/jpeg",
        lib.id,
        Some("h1"),
    )
    .await
    .expect("insert first");
    let id2 = db::insert_uploaded_photo(
        &pool,
        p,
        "sample.jpg",
        200,
        "image/jpeg",
        lib.id,
        Some("h2"),
    )
    .await
    .expect("insert conflict");

    assert_eq!(id1, id2);
    let photo = db::get_photo_by_id(&pool, id1)
        .await
        .expect("get photo")
        .expect("photo exists");
    assert_eq!(photo.file_size, 200);
    assert_eq!(photo.file_hash.as_deref(), Some("h2"));

    drop(pool);
    let _ = std::fs::remove_file(db_path);
}

#[tokio::test]
async fn ai_tag_counts_are_scoped_by_libraries() {
    let (db_url, db_path) = temp_db_url();
    std::fs::File::create(&db_path).expect("create db file");
    let pool = db::init_pool(&db_url).await.expect("init db");

    let u1 = db::create_user(
        &pool,
        &CreateUser {
            username: "tag_u1".to_string(),
            password: "ignored".to_string(),
            role: Some("user".to_string()),
        },
        "hash1",
    )
    .await
    .expect("create user1");
    let u2 = db::create_user(
        &pool,
        &CreateUser {
            username: "tag_u2".to_string(),
            password: "ignored".to_string(),
            role: Some("user".to_string()),
        },
        "hash2",
    )
    .await
    .expect("create user2");

    let lib1 = db::create_library(
        &pool,
        &CreateLibrary {
            name: "lib1".to_string(),
            path: "/tmp/lib1".to_string(),
        },
        u1.id,
    )
    .await
    .expect("create lib1");
    let lib2 = db::create_library(
        &pool,
        &CreateLibrary {
            name: "lib2".to_string(),
            path: "/tmp/lib2".to_string(),
        },
        u2.id,
    )
    .await
    .expect("create lib2");

    let p1 = db::upsert_photo(
        &pool,
        &db::InsertPhoto {
            file_path: "/tmp/lib1/a.jpg".to_string(),
            file_name: "a.jpg".to_string(),
            file_size: 1,
            file_hash: Some("h1".to_string()),
            mime_type: "image/jpeg".to_string(),
            width: None,
            height: None,
            taken_at: None,
            latitude: None,
            longitude: None,
            camera_make: None,
            camera_model: None,
            library_id: lib1.id,
        },
    )
    .await
    .expect("p1");
    let p2 = db::upsert_photo(
        &pool,
        &db::InsertPhoto {
            file_path: "/tmp/lib2/b.jpg".to_string(),
            file_name: "b.jpg".to_string(),
            file_size: 1,
            file_hash: Some("h2".to_string()),
            mime_type: "image/jpeg".to_string(),
            width: None,
            height: None,
            taken_at: None,
            latitude: None,
            longitude: None,
            camera_make: None,
            camera_model: None,
            library_id: lib2.id,
        },
    )
    .await
    .expect("p2");

    let tag_id = db::upsert_tag(&pool, "landscape", "scene")
        .await
        .expect("tag");
    db::add_photo_tag(&pool, p1, tag_id, 0.9)
        .await
        .expect("tag p1");
    db::add_photo_tag(&pool, p2, tag_id, 0.8)
        .await
        .expect("tag p2");

    let tags_user1 = db::get_tags_with_counts_in_libraries(&pool, &[lib1.id])
        .await
        .expect("tags scoped");
    assert_eq!(tags_user1.len(), 1);
    assert_eq!(tags_user1[0].name, "landscape");
    assert_eq!(tags_user1[0].photo_count, 1);

    let (photos, total) = db::get_photos_by_tag_in_libraries(&pool, tag_id, &[lib1.id], 0, 50)
        .await
        .expect("photos scoped");
    assert_eq!(total, 1);
    assert_eq!(photos.len(), 1);
    assert_eq!(photos[0].id, p1);

    drop(pool);
    let _ = std::fs::remove_file(db_path);
}

#[test]
fn dhash_is_stable_and_splits_distinct_patterns() {
    let mut left_bright = GrayImage::new(16, 16);
    let mut right_bright = GrayImage::new(16, 16);

    for y in 0..16 {
        for x in 0..16 {
            left_bright.put_pixel(x, y, Luma([if x < 8 { 255 } else { 0 }]));
            right_bright.put_pixel(x, y, Luma([if x < 8 { 0 } else { 255 }]));
        }
    }

    let img_a = DynamicImage::ImageLuma8(left_bright);
    let img_b = DynamicImage::ImageLuma8(right_bright);

    let hash_a1 = dedup::compute_dhash_from_image(&img_a);
    let hash_a2 = dedup::compute_dhash_from_image(&img_a);
    let hash_b = dedup::compute_dhash_from_image(&img_b);

    assert_eq!(hash_a1.len(), 16);
    assert_eq!(hash_a1, hash_a2);
    assert_ne!(hash_a1, hash_b);
}
