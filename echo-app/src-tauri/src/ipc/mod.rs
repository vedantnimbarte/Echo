/*!
 * SOURCE OF TRUTH KEYWORDS: ipc, factory, execute, CommandSpec
 * WHAT:  The IPC boundary layer: the one function every command passes through.
 * WHY:   A module rather than a file so the boundary is a place in the tree
 *        rather than a convention people remember. Handlers live in commands/;
 *        what is true of ALL of them lives here.
 * WHERE: Used by commands/; depends on registry/ and error.rs, never the
 *        reverse.
 */

pub mod factory;
