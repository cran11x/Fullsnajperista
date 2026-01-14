use anyhow::Result;

/// Smoke test for SOL price fetching + conversions.
/// Kept as a CLI tool entry (`test-sol-price`) but moved out of `main.rs`.
pub async fn run() -> Result<()> {
    println!("🧪 Testing SOL price fetching...\n");

    // Test 1: Get cached price (before refresh)
    println!("1️⃣  Testing cached price (before refresh)...");
    let cached_before = crate::utils::get_cached_sol_price();
    println!("   Cached price: ${:.2}", cached_before);

    // Test 2: Refresh SOL price
    println!("\n2️⃣  Refreshing SOL price...");
    crate::utils::refresh_sol_price_if_needed().await;

    // Test 3: Get cached price (after refresh)
    println!("\n3️⃣  Testing cached price (after refresh)...");
    let cached_after = crate::utils::get_cached_sol_price();
    println!("   Cached price: ${:.2}", cached_after);

    if (cached_after - cached_before).abs() > 0.01 {
        println!("   ✅ Price was updated!");
    } else if cached_before == 150.0 {
        println!("   ✅ Price was updated from default!");
    } else {
        println!("   ℹ️  Price unchanged (cache was fresh)");
    }

    // Test 4: Test conversion functions
    println!("\n4️⃣  Testing conversion functions...");
    let test_sol = 100.0;
    let test_usd = 15000.0;

    let sol_to_usd_result = crate::utils::sol_to_usd(test_sol);
    let usd_to_sol_result = crate::utils::usd_to_sol(test_usd);

    println!("   {} SOL = ${:.2} USD", test_sol, sol_to_usd_result);
    println!("   ${:.2} USD = {:.4} SOL", test_usd, usd_to_sol_result);

    // Verify conversion is correct
    let back_to_sol = crate::utils::usd_to_sol(sol_to_usd_result);
    if (back_to_sol - test_sol).abs() < 0.01 {
        println!("   ✅ Conversion is accurate!");
    } else {
        println!(
            "   ⚠️  Conversion round-trip error: {:.4} != {:.4}",
            back_to_sol, test_sol
        );
    }

    // Test 5: Test live price fetch (direct API call)
    println!("\n5️⃣  Testing live price fetch (direct API call)...");
    match crate::utils::fetch_sol_price_usd().await {
        Ok(price) => {
            println!("   ✅ Live SOL price: ${:.2}", price);
            println!(
                "   Cached price: ${:.2}",
                crate::utils::get_cached_sol_price()
            );
            if (price - crate::utils::get_cached_sol_price()).abs() < 0.01 {
                println!("   ✅ Cache matches live price!");
            } else {
                println!("   ⚠️  Cache differs from live price!");
            }
        }
        Err(e) => {
            println!("   ❌ Error fetching live price: {}", e);
        }
    }

    // Test 6: Test MC conversion (18k USD example)
    println!("\n6️⃣  Testing MC conversion (18k USD example)...");
    let mc_usd = 18000.0;
    let mc_sol = crate::utils::usd_to_sol(mc_usd);
    println!(
        "   ${:.0} USD = {:.2} SOL (at ${:.2}/SOL)",
        mc_usd,
        mc_sol,
        crate::utils::get_cached_sol_price()
    );

    // Convert back
    let mc_usd_back = crate::utils::sol_to_usd(mc_sol);
    println!("   {:.2} SOL = ${:.0} USD", mc_sol, mc_usd_back);

    if (mc_usd_back - mc_usd).abs() < 1.0 {
        println!("   ✅ MC conversion is accurate!");
    } else {
        println!(
            "   ⚠️  MC conversion error: ${:.0} != ${:.0}",
            mc_usd_back, mc_usd
        );
    }

    println!("\n✅ Test completed!");
    Ok(())
}


