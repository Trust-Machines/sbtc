-- This table holds the canonical chain information for Bitcoin and Stacks
-- chains keyed by the bitcoin chain tip.
CREATE TABLE sbtc_signer.canonical_chains (
    run_id INT,
    bitcoin_chain_tip BYTEA,
    bitcoin_block_hash BYTEA,
    bitcoin_block_height BIGINT,
    stacks_block_hash BYTEA,
    stacks_block_height BIGINT
);

-- Indexes to support common queries on the canonical chains table.
CREATE INDEX ix_canonical_chains_run_id ON sbtc_signer.canonical_chains(run_id);
CREATE INDEX ix_canonical_chains_bitcoin_chain_tip ON sbtc_signer.canonical_chains(bitcoin_chain_tip);
CREATE INDEX ix_canonical_chains_bitcoin_block_hash ON sbtc_signer.canonical_chains(bitcoin_block_hash);
CREATE INDEX ix_canonical_chains_bitcoin_block_height ON sbtc_signer.canonical_chains(bitcoin_block_height);
CREATE INDEX ix_canonical_chains_stacks_block_hash ON sbtc_signer.canonical_chains(stacks_block_hash);
CREATE INDEX ix_canonical_chains_stacks_block_height ON sbtc_signer.canonical_chains(stacks_block_height);

-- New sequence which will be used to generate the run_id for each materialized
-- view canonical chain.
CREATE SEQUENCE sbtc_signer.canonical_chains_run_id_seq;

-- Function to materialize the canonical chains for a given bitcoin chain tip
-- and the maximum depth of the bitcoin blockchain to consider.
--
-- This function returns the number of rows written to the `canonical_chains`
-- table upon success, and `-1` if rows already exist for the given chain tip.
CREATE OR REPLACE FUNCTION sbtc_signer.materialize_canonical_chains(chain_tip BYTEA, max_depth INT)
RETURNS INTEGER AS $$
DECLARE
    rows_written INTEGER;
	existing_rows INTEGER;
	new_run_id BIGINT;
BEGIN
	-- Get the next value for the run_id
    new_run_id := nextval('sbtc_signer.canonical_chains_run_id_seq');

	-- Check if rows exist with the given chain_tip
    SELECT COUNT(*) INTO existing_rows
    FROM sbtc_signer.canonical_chains
    WHERE bitcoin_chain_tip = chain_tip;

    -- If rows exist, return an error code
    IF existing_rows > 0 THEN
        RETURN -1; -- Error code indicating rows already exist
    END IF;

	-- Materialize the canonical chains from the given bitcoin chain tip.
    WITH RECURSIVE 
    bitcoin AS (
        SELECT 
            block_hash,
            parent_hash,
            block_height,
			1 as depth
        FROM sbtc_signer.bitcoin_blocks
        WHERE block_hash = chain_tip

        UNION ALL

        SELECT 
            parent.block_hash,
            parent.parent_hash,
            parent.block_height,
			last.depth + 1
        FROM sbtc_signer.bitcoin_blocks parent
        JOIN bitcoin last ON parent.block_hash = last.parent_hash
		WHERE last.depth < max_depth
    ),
    stacks AS (
        (SELECT 
            blocks.block_hash,
            blocks.parent_hash,
            blocks.block_height,
            blocks.bitcoin_anchor
        FROM sbtc_signer.stacks_blocks blocks
        JOIN bitcoin ON blocks.bitcoin_anchor = bitcoin.block_hash
        ORDER BY bitcoin.block_height DESC, bitcoin.block_hash DESC, blocks.block_height DESC, blocks.block_hash DESC
        LIMIT 1)

        UNION ALL

        SELECT 
            parent.block_hash,
            parent.parent_hash,
            parent.block_height,
            parent.bitcoin_anchor
        FROM sbtc_signer.stacks_blocks parent
        JOIN stacks last ON parent.block_hash = last.parent_hash
        JOIN bitcoin ON bitcoin.block_hash = parent.bitcoin_anchor
    )
    INSERT INTO sbtc_signer.canonical_chains (
		run_id,
        bitcoin_chain_tip,
        bitcoin_block_hash,
        bitcoin_block_height,
        stacks_block_hash,
        stacks_block_height
    )
    SELECT 
		new_run_id,
        chain_tip,
        bb.block_hash AS bitcoin_block_hash,
        bb.block_height AS bitcoin_block_height,
        sb.block_hash AS stacks_block_hash,
        sb.block_height AS stacks_block_height
    FROM bitcoin bb
    LEFT JOIN stacks sb ON sb.bitcoin_anchor = bb.block_hash
    ORDER BY bb.block_height DESC, sb.block_height DESC;

    GET DIAGNOSTICS rows_written = ROW_COUNT;
    RETURN rows_written;
END;
$$ LANGUAGE plpgsql;
