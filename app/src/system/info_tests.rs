use byte_unit::Byte;

use crate::{terminal::model::test_utils::TestBlockBuilder, test_util::mock_blockgrid};

use super::*;

#[test]
fn test_memory_usage_stats_construction() {
    let total_application_usage_bytes = 1024;
    let mut stats = MemoryUsageStats::new(Byte::from_u64(total_application_usage_bytes));

    let now = Local::now();

    let mut block_with_content = TestBlockBuilder::new().build();
    block_with_content.set_prompt_and_command_grid(mock_blockgrid("line1\nline2"));
    block_with_content.set_output_grid(mock_blockgrid("line3"));
    block_with_content.update_last_painted_at(now);

    let inactive_5m_block = TestBlockBuilder::new().build();
    inactive_5m_block.update_last_painted_at(now - chrono::Duration::minutes(10));

    let inactive_1h_block1 = TestBlockBuilder::new().build();
    inactive_1h_block1.update_last_painted_at(now - chrono::Duration::minutes(70));

    let inactive_1h_block2 = TestBlockBuilder::new().build();
    inactive_1h_block2.update_last_painted_at(now - chrono::Duration::minutes(70));

    let blocks = [
        block_with_content,
        inactive_5m_block,
        inactive_1h_block1,
        inactive_1h_block2,
        TestBlockBuilder::new().build(),
    ];

    stats.add_blocks(now, blocks.iter());

    assert_eq!(
        stats.total_application_usage_bytes,
        total_application_usage_bytes as usize
    );
    assert_eq!(stats.total_blocks, 5);
    assert_eq!(stats.total_lines, 3);

    assert_eq!(stats.active_block_stats.num_blocks, 1);
    assert_eq!(stats.active_block_stats.num_lines, 3);

    assert_eq!(stats.inactive_5m_stats.num_blocks, 1);
    assert_eq!(stats.inactive_5m_stats.num_lines, 0);

    assert_eq!(stats.inactive_1h_stats.num_blocks, 2);
    assert_eq!(stats.inactive_1h_stats.num_lines, 0);

    assert_eq!(stats.inactive_24h_stats.num_blocks, 1);
    assert_eq!(stats.inactive_24h_stats.num_lines, 0);
}

#[test]
fn resource_history_buffer_keeps_recent_samples() {
    let mut history = ResourceHistoryBuffer::new();

    for index in 0..RESOURCE_HISTORY_SAMPLE_COUNT + 2 {
        history.push(ResourceUsageSample {
            cpu_usage: index as f32,
            gpu_usage: Some(index as f32 / 2.),
            memory_footprint_bytes: index as u64,
            memory_usage: Some(index as f32 / 4.),
        });
    }

    let samples = history.iter().copied().collect::<Vec<_>>();
    assert_eq!(samples.len(), RESOURCE_HISTORY_SAMPLE_COUNT);
    assert_eq!(samples[0].cpu_usage, 2.);
    assert_eq!(
        history
            .last()
            .expect("history should have samples")
            .cpu_usage,
        (RESOURCE_HISTORY_SAMPLE_COUNT + 1) as f32
    );
}
