use actix_web::{web, App, HttpResponse, HttpServer, Result};
use serde::{Deserialize, Serialize};
use sqlx::{MySqlPool, Pool, MySql};
use rust_decimal::Decimal;
use dotenvy::dotenv;
use std::{env};
use webbrowser;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CalculateTuitionFormParams {
    // We need to use Option<...> because sometimes the fields can be empty from form submission.
    first_name: Option<String>,
    last_name: Option<String>,
    num_credits: Option<String>,
    new_student: Option<String>,
    orientation: Option<String>,
    student_type: Option<String>,
    student_studies: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct LookupFormParams {
    first_name: Option<String>,
    last_name: Option<String>,
}

pub struct TypeSafeLookupFormParams {
    firstName: String,
    lastName: String,
}

enum StudentResidency {
    In,
    Out,
}

enum StudentStudies {
    Undergraduate,
    Graduate,
}

pub struct TypeSafeParameters {
    first_name: String,
    last_name: String,
    num_credits: u8,
    new_student: bool,
    orientation: bool,
    student_type: StudentResidency,
    student_studies: StudentStudies,
}

#[derive(Debug, Clone)]
struct AppState {
    app_name: String,
    conn: Pool<MySql>,
}

async fn lookup(state: web::Data<AppState>, params: web::Form<LookupFormParams>) -> Result<HttpResponse> {
    let pool = &state.conn;

    let type_safe_params = TypeSafeLookupFormParams {
        firstName: match &params.first_name {
            Some(val) => val.to_string(),
            None => {
                return error("First name not provided").await;
            }
        },
        lastName: match &params.last_name {
            Some(val) => val.to_string(),
            None => {
                return error("Last name not provided").await;
            }
        }
    };

    #[derive(sqlx::FromRow)]
    struct UserTuition {
        #[allow(non_snake_case)]
        FirstName: String,
        #[allow(non_snake_case)]
        LastName: String,
        #[allow(non_snake_case)]
        TuitionCost: rust_decimal::Decimal,
    }

    // Get the row from the database.
    let sql_result = sqlx::query_as::<_, UserTuition>
    (
        "select FirstName, LastName, TuitionCost
        from UserTuition
        where FirstName = ?
        and LastName = ?"
    )
    .bind(&type_safe_params.firstName)
    .bind(&type_safe_params.lastName)
    .fetch_one(pool).await;

    let user_tuition = match sql_result {
        Ok(val) => val,
        Err(why) => {
            // 'why' is a sqlx::Error type.
            return error(&format!("Error while accessing database: {}", why.to_string())).await;
        }
    };

    // Print the row!
    let lookup = "
        <html>
            <head>
                <link rel=\"stylesheet\" type=\"text/css\" href=\"style.css\" />
                <meta charset=utf-8>
            </head>
            <body>
                <section>
                    <table>
                        <tr>
                            <th>Name</th>
                            <th>Tuition</th>
                        </tr>
                        <tr>
                            <td>".to_owned() + &format!("{} {}", user_tuition.FirstName, user_tuition.LastName) + "</td>
                            <td>$" + &user_tuition.TuitionCost.to_string() + "</td>
                        </tr>
                    </table>
                </section>
            </body>
        </html>
    ";

    Ok(HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(lookup)
    )
}

async fn error(console_msg: &str) -> Result<HttpResponse> {
    println!("{}", console_msg);
    
    return 
        Ok(HttpResponse::Ok()
            .content_type("text/html; charset=utf-8")
            .body(include_str!("htdoc/error.html")));
}

async fn calculate(state: web::Data<AppState>, params: web::Form<CalculateTuitionFormParams>) -> Result<HttpResponse> {  

    let pool = &state.conn;

    #[derive(sqlx::FromRow)]
    struct TuitionCosts {
        #[allow(non_snake_case)]
        CreditsCost: rust_decimal::Decimal,
        #[allow(non_snake_case)]
        NonresidencyFee: rust_decimal::Decimal,
    }

    #[derive(sqlx::FromRow)]
    struct OrientationFee {
        #[allow(non_snake_case)]
        Fee: rust_decimal::Decimal,
    }

    // Check our values.
    // Build our typesafe parameters.
    let type_safe_parameters = TypeSafeParameters {
        first_name: match &params.first_name {
            Some(val) => val.to_string(),
            None => {
                return error("No first name was provided!").await;
            }
        },
        last_name: match &params.last_name {
            Some(val) => val.to_string(),
            None => {
                return error("No last name was provided!").await;
            }
        },
        num_credits: match &params.num_credits {
            Some(val) => val.parse::<u8>().unwrap(),
            None => {
                return error("No credits were provided!").await;
            }
        },
        new_student: match &params.new_student {
            Some(val) => {
                if val.eq("on") {true} else {false}
            },
            None => false
        },
        orientation: match &params.orientation {
            Some(val) => {
                if val.eq("on") {true} else {false}
            },
            None => false
        },
        student_type: match &params.student_type {
            Some(val) => {
                if val.eq("resident") 
                    {StudentResidency::In} 
                else if val.eq("nonresident") 
                    {StudentResidency::Out} 
                else 
                    {StudentResidency::Out}
            }
            None => {
                return error("User must be either a nonresident or resident.").await;
            }
        },
        student_studies: match &params.student_studies {
            Some(val) => {
                if val.eq("undergraduate") 
                    {StudentStudies::Undergraduate} 
                else if val.eq("nonresident") 
                    {StudentStudies::Graduate} 
                else 
                    {StudentStudies::Undergraduate}
            }
            None => {
                return error("User must be either a undergraduate or graduate.").await;
            }
        }
    };

    // Get the cost per credit from the database by using prepared statements.
    let sql_result = sqlx::query_as::<_, TuitionCosts>(
    "SELECT CreditCosts.CreditsCost, CreditCosts.NonresidencyFee
    FROM CreditCosts
    WHERE CreditCosts.Studies = ?
    AND CreditCosts.Residency = ?")
        .bind(&params.student_studies)
        .bind(&params.student_type)
        .fetch_one(pool).await;

    let tuition_cost = match sql_result {
        Ok(val) => val,
        Err(why) => {
            // If there is an error, then throw the html webpage error and exit.
            return error(&format!("Error while accessing database: {}", why.to_string())).await;
        }
    };
    // Also get the orientation fee, if the user checked it.
    let mut orientation_fee = OrientationFee { Fee: Decimal::new(000, 2) };
    if type_safe_parameters.orientation {
        // Get the cost per credit from the database by using prepared statements.
        let sql_result_orientation_fee = sqlx::query_as::<_, OrientationFee>(
        "SELECT Fee
        FROM orientation_fee")
            .fetch_one(pool).await;
        orientation_fee = match sql_result_orientation_fee {
            Ok(val) => val,
            Err(why) => {
                // If there is an error, then throw the html webpage error and exit.
                return error(&format!("Error while accessing database: {}", why.to_string())).await;
            }
        };
    }

    // Multiplty the cost per credit by the credits
    let total = tuition_cost.CreditsCost * Decimal::from(type_safe_parameters.num_credits) + tuition_cost.NonresidencyFee + orientation_fee.Fee;
    println!("The total tuition cost is ${}", total);

    // Create the HTML table of the calculation that took place
    let table = " 
    <!DOCTYPE html>
    <html>
        <head>
            <link rel=\"stylesheet\" type=\"text/css\" href=\"style.css\" />
        </head>
        <body>
            <section>
                <h1>Tuition Results</h1>
                <p>Name: ".to_owned() + &format!("{} {}", type_safe_parameters.first_name, type_safe_parameters.last_name) + "</p>
                <table>
                    <tr>
                        <th>Residency</th>
                        <th>Studies</th>
                        <th>New Student Status</th>
                        <th>Orientation Fee</th>
                        <th>Non-Residency Fee</th>
                        <th>Number of Credits</th>
                        <th>Costs per Credit</th>
                    </tr>
                    <tr>
                        <td>" + match type_safe_parameters.student_type { StudentResidency::In => "Resident", StudentResidency::Out => "Non-Resident" } + "</td>
                        <td>" + match type_safe_parameters.student_studies { StudentStudies::Undergraduate => "Undergraduate", StudentStudies::Graduate => "Graduate" } + "</td>
                        <td>" + match type_safe_parameters.new_student { true => "Yes", false => "No" } + "</td>
                        <td>$" + &orientation_fee.Fee.to_string() + "</td>
                        <td>$" + &tuition_cost.NonresidencyFee.to_string() + "</td>
                        <td>" + &type_safe_parameters.num_credits.to_string() + "</td>
                        <td>$" + &tuition_cost.CreditsCost.to_string() + "</td>
                    </tr>
                </table>
                <p><b>Total: </b> $" + &total.to_string() + "</p>
            </section>
        </body>
    </html>";

    #[derive(sqlx::FromRow)]
    struct User { FirstName: String, LastName: String, }

    // See if it already exists. If it is, update it.
    let user_exists = match sqlx::query_as::<_, User>(
        "select FirstName, LastName
        from UserTuition
        where FirstName = ?
        and LastName = ?"
    )
    .bind(&type_safe_parameters.first_name)
    .bind(&type_safe_parameters.last_name)
    .fetch_one(pool)
    .await {
        Ok(_) => {true},
        Err(_) => {false}
    };

    // Add the result to our user table.
    if !user_exists {
        match sqlx::query(
            "insert into UserTuition 
            (FirstName, LastName, TuitionCost) 
            VALUES 
            (?, ?, ?)")
        .bind(type_safe_parameters.first_name)
        .bind(type_safe_parameters.last_name)
        .bind(total)
        .execute(pool)
        .await {
            Ok(_val) => {},
            Err(why) => {
                return error(&format!("Error while inserting to the database: {}", why.to_string())).await;
            }
        };
    } else {
        // Or, update the result.
        match sqlx::query(
            "update UserTuition 
            set TuitionCost = ?
            where FirstName = ?
            and LastName = ?")
        .bind(total)
        .bind(type_safe_parameters.first_name)
        .bind(type_safe_parameters.last_name)
        .execute(pool)
        .await {
            Ok(_val) => {},
            Err(why) => {
                return error(&format!("Error while updating the database: {}", why.to_string())).await;
            }
        };
    }


    Ok(HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(table))
}

async fn index() -> Result<HttpResponse> {
    Ok(HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(include_str!("htdoc/index.html")))
}

async fn style() -> Result<HttpResponse> {
    Ok(HttpResponse::Ok()
        .content_type("text/css")
        .body(include_str!("htdoc/style.css")))
}

fn app_config(config: &mut web::ServiceConfig) {
    
    config.service(
        web::scope("")
            .route("/style.css", web::get().to(style))
            .service(web::resource("/").route(web::get().to(index)))
            .service(web::resource("/lookup").route(web::post().to(lookup)))
            .service(web::resource("/calculate").route(web::post().to(calculate))),
    );
}

#[actix_web::main]
async fn main() -> Result<(), sqlx::Error> {

    // Get our environment variables.
    dotenv().ok();
    let db_string = env::var("DATABASE_URL").expect("Database connection URL not found in dotenv file.");
    let host = env::var("HOST").expect("Host URL not found in dotenv file.");
    let port = env::var("PORT").expect("Port number not found in dotenv file.");
    let server_url = format!("{}:{}", host, port);
    
    // Start the DB connection with sqlx.
    let pool = MySqlPool::connect(&db_string).await?;
    println!("Connected to the database at {}.", db_string);

    // Add the connection to our app state so it is shared.
    let state = AppState {
        app_name: String::from("Tuition Calculator"),
        conn: pool,
    };

    println!("Server started at {}. Application name: \"{}\"", server_url, state.app_name);
    webbrowser::open(&format!("http://{}", server_url)).unwrap();
    // Execute our http server application.
    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(state.clone()))
            .configure(app_config)
    })
    .bind(server_url.clone())?
    .run()
    .await.expect("Error creating HTTP server.");

    // Satisfy the () in the Result.
    Ok(())
}