// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use viper::{self, Viper, Stmt, Expr, VerificationError, CfgMethod};
use viper::{Domain, Field, Function, Predicate, Method};
use viper::AstFactory;
use rustc::mir;
use rustc::ty;
use prusti_interface::environment::ProcedureImpl;
use prusti_interface::data::ProcedureDefId;
use prusti_interface::environment::Environment;
use prusti_interface::environment::Procedure;
use std::collections::HashMap;
use viper::CfgBlockIndex;
use prusti_interface::environment::BasicBlockIndex;
use rustc::mir::TerminatorKind;
use viper::Successor;
use rustc::middle::const_val::{ConstInt, ConstVal};
use encoder::Encoder;
use encoder::borrows::compute_procedure_contract;
use encoder::utils::*;
use rustc_data_structures::indexed_vec::Idx;
use rustc::ty::layout::LayoutOf;

pub struct ProcedureEncoder<'p, 'v: 'p, 'r: 'v, 'a: 'r, 'tcx: 'a> {
    encoder: &'p Encoder<'v, 'r, 'a, 'tcx>,
    proc_def_id: ProcedureDefId,
    procedure: &'p ProcedureImpl<'a, 'tcx>,
    mir: &'p mir::Mir<'tcx>,
    cfg_method: CfgMethod<'v, 'p>
}

impl<'p, 'v: 'p, 'r: 'v, 'a: 'r, 'tcx: 'a> ProcedureEncoder<'p, 'v, 'r, 'a, 'tcx> {
    pub fn new(encoder: &'p Encoder<'v, 'r, 'a, 'tcx>, procedure: &'p ProcedureImpl<'a, 'tcx>) -> Self {
        let mut cfg_method = encoder.cfg_factory().new_cfg_method(
            // method name
            encoder.encode_procedure_name(procedure.get_id()),
            // formal args
            vec![],
            // formal returns
            vec![],
            // local vars
            vec![],
        );

        ProcedureEncoder {
            encoder,
            proc_def_id: procedure.get_id(),
            procedure,
            mir: procedure.get_mir(),
            cfg_method
        }
    }

    fn encode_statement(&mut self, stmt: &mir::Statement<'tcx>) -> Vec<Stmt<'v>> {
        debug!("Encode statement '{:?}'", stmt);
        let ast = self.encoder.ast_factory();
        let mut stmts: Vec<Stmt> = vec![];

        match stmt.kind {
            mir::StatementKind::StorageDead(_) |
            mir::StatementKind::StorageLive(_) |
            mir::StatementKind::EndRegion(_) => stmts,

            mir::StatementKind::Assign(ref lhs, ref rhs) => {
                let (encoded_lhs, ty, _) = self.encode_place(lhs);
                match rhs {
                    &mir::Rvalue::Use(ref operand) => {
                        let (encoded_value, effects_before, effects_after) = self.encode_operand(operand);
                        stmts.extend(effects_before);
                        stmts.push(
                            ast.assign(
                                encoded_lhs,
                                encoded_value,
                                self.is_place_encoded_as_local_var(lhs)
                            )
                        );
                        stmts.extend(effects_after);
                        stmts
                    },

                    &mir::Rvalue::BinaryOp(op, ref left, ref right) => {
                        let (encoded_left, effects_after_left) = self.eval_operand(left);
                        let (encoded_right, effects_after_right) = self.eval_operand(right);
                        let field = self.encoder.encode_value_field(ty);
                        let encoded_value = self.encode_bin_op_value(op, encoded_left, encoded_right);
                        stmts.push(
                            ast.field_assign(
                                ast.field_access(
                                    encoded_lhs,
                                    field
                                ),
                                encoded_value,
                            )
                        );
                        stmts.extend(effects_after_left);
                        stmts.extend(effects_after_right);
                        stmts
                    },

                    &mir::Rvalue::CheckedBinaryOp(op, ref left, ref right) => {
                        let (encoded_left, effects_after_left) = self.eval_operand(left);
                        let (encoded_right, effects_after_right) = self.eval_operand(right);
                        let encoded_value = self.encode_bin_op_value(op, encoded_left, encoded_right);
                        let encoded_check = self.encode_bin_op_check(op, encoded_left, encoded_right);
                        let elems = if let ty::TypeVariants::TyTuple(ref x, _) = ty.sty { x } else { unreachable!() };
                        let value_field = self.encoder.encode_ref_field("tuple_0");
                        let value_field_value = self.encoder.encode_value_field(elems[0]);
                        let check_field = self.encoder.encode_ref_field("tuple_1");
                        let check_field_value = self.encoder.encode_value_field(elems[1]);
                        stmts.push(
                            ast.field_assign(
                                ast.field_access(
                                    ast.field_access(
                                        encoded_lhs,
                                        value_field
                                    ),
                                    value_field_value
                                ),
                                encoded_value,
                            )
                        );
                        stmts.push(
                            ast.field_assign(
                                ast.field_access(
                                    ast.field_access(
                                        encoded_lhs,
                                        check_field
                                    ),
                                    check_field_value
                                ),
                                encoded_check,
                            )
                        );
                        stmts.extend(effects_after_left);
                        stmts.extend(effects_after_right);
                        stmts
                    },

                    &mir::Rvalue::UnaryOp(op, ref operand) => unimplemented!("{:?}", rhs),

                    &mir::Rvalue::NullaryOp(op, ref ty) => unimplemented!("{:?}", rhs),

                    &mir::Rvalue::Discriminant(ref src) => {
                        let discr_var_name = self.cfg_method.add_fresh_local_var(ast.ref_type());
                        let discr_var = ast.local_var(&discr_var_name, ast.ref_type());
                        stmts.extend(
                            self.encode_allocation(discr_var, ty, true)
                        );
                        let int_field = self.encoder.encode_value_field(ty);
                        let discr_field = self.encoder.encode_discriminant_field().1;
                        let (encoded_src, _, _) = self.encode_place(src);
                        stmts.push(
                            ast.assign(
                                encoded_lhs,
                                discr_var,
                                self.is_place_encoded_as_local_var(lhs)
                            )
                        );
                        stmts.push(
                            ast.field_assign(
                                ast.field_access(
                                    encoded_lhs,
                                    int_field
                                ),
                                ast.field_access(
                                    encoded_src,
                                    discr_field
                                )
                            )
                        );
                        stmts
                    }

                    &mir::Rvalue::Aggregate(ref aggregate, ref operands) => {
                        let (encoded_value, before_stmts) = self.encode_assign_aggregate(ty, aggregate, operands);
                        stmts.extend(before_stmts);
                        stmts.push(
                            ast.assign(
                                encoded_lhs,
                                encoded_value,
                                self.is_place_encoded_as_local_var(lhs)
                            )
                        );
                        stmts
                    }

                    &mir::Rvalue::Ref(ref region, borrow_kind, ref place) => {
                        let ref_var_name = self.cfg_method.add_fresh_local_var(ast.ref_type());
                        let ref_var = ast.local_var(&ref_var_name, ast.ref_type());
                        stmts.extend(
                            self.encode_allocation(ref_var, ty, true)
                        );
                        let field = self.encoder.encode_value_field(ty);
                        let (encoded_value, _, _) = self.encode_place(place);
                        stmts.push(
                            ast.assign(
                                encoded_lhs,
                                ref_var,
                                self.is_place_encoded_as_local_var(lhs)
                            )
                        );
                        stmts.push(
                            ast.field_assign(
                                ast.field_access(
                                    encoded_lhs,
                                    field
                                ),
                                encoded_value,
                            )
                        );
                        stmts
                    }

                    ref x => unimplemented!("{:?}", x)
                }
            },

            ref x => unimplemented!("{:?}", x)
        }
    }

    fn encode_terminator(&mut self, term: &mir::Terminator<'tcx>,
                         cfg_blocks: &HashMap<BasicBlockIndex, CfgBlockIndex>,
                         spec_cfg_block: CfgBlockIndex,
                         abort_cfg_block: CfgBlockIndex,
                         return_cfg_block: CfgBlockIndex) -> (Vec<Stmt<'v>>, Successor<'v>) {
        trace!("Encode terminator '{:?}'", term.kind);
        let ast = self.encoder.ast_factory();
        let mut stmts: Vec<Stmt> = vec![];

        match term.kind {
            TerminatorKind::Return => {
                (stmts, Successor::Goto(return_cfg_block))
            },

            TerminatorKind::Goto { target } => {
                let target_cfg_block = cfg_blocks.get(&target).unwrap_or(&spec_cfg_block);
                (stmts, Successor::Goto(*target_cfg_block))
            },

            TerminatorKind::SwitchInt { ref targets, ref discr, ref values, switch_ty } => {
                trace!("SwitchInt ty '{:?}', discr '{:?}', values '{:?}'", switch_ty, discr, values);
                let mut cfg_targets: Vec<(Expr, CfgBlockIndex)> = vec![];
                let discr_var = if (switch_ty.sty == ty::TypeVariants::TyBool) {
                    let discr_var_name = self.cfg_method.add_fresh_local_var(ast.bool_type());
                    ast.local_var(&discr_var_name, ast.bool_type())
                } else {
                    let discr_var_name = self.cfg_method.add_fresh_local_var(ast.int_type());
                    ast.local_var(&discr_var_name, ast.int_type())
                };
                let (discr_tmp_val, effect_stmts) = self.eval_operand(discr);
                stmts.push(
                    ast.local_var_assign(
                        discr_var,
                        discr_tmp_val
                    )
                );
                stmts.extend(
                    effect_stmts
                );
                for (i, value) in values.iter().enumerate() {
                    let target = targets[i as usize];
                    // Convert int to bool, if required
                    let viper_guard = if (switch_ty.sty == ty::TypeVariants::TyBool) {
                        if const_int_is_zero(value) {
                            // If discr is 0 (false)
                            ast.not(discr_var)
                        } else {
                            // If discr is not 0 (true)
                            discr_var
                        }
                    } else {
                        ast.eq_cmp(
                            discr_var,
                            self.encoder.eval_const_int(value)
                        )
                    };
                    let target_cfg_block = cfg_blocks.get(&target).unwrap_or(&spec_cfg_block);
                    cfg_targets.push((viper_guard, *target_cfg_block))
                }
                let default_target = targets[values.len()];
                let cfg_default_target = cfg_blocks.get(&default_target).unwrap_or(&spec_cfg_block);
                (stmts, Successor::GotoSwitch(cfg_targets, *cfg_default_target))
            },

            TerminatorKind::Unreachable => {
                (stmts, Successor::Unreachable)
            },

            TerminatorKind::Abort => {
                (stmts, Successor::Goto(abort_cfg_block))
            },

            TerminatorKind::Drop { ref target, unwind, .. } => {
                let target_cfg_block = cfg_blocks.get(&target).unwrap_or(&spec_cfg_block);
                (stmts, Successor::Goto(*target_cfg_block))
            },

            TerminatorKind::FalseEdges { ref real_target, ref imaginary_targets } => {
                let target_cfg_block = cfg_blocks.get(&real_target).unwrap_or(&spec_cfg_block);
                (stmts, Successor::Goto(*target_cfg_block))
            },

            TerminatorKind::FalseUnwind { real_target, unwind } => {
                let target_cfg_block = cfg_blocks.get(&real_target).unwrap_or(&spec_cfg_block);
                (stmts, Successor::Goto(*target_cfg_block))
            },

            TerminatorKind::DropAndReplace { ref target, unwind, ref location, ref value } => {
                let (encoded_loc, _, _) = self.encode_place(location);
                let (encoded_value, effects_before, effects_after) = self.encode_operand(value);
                stmts.extend(effects_before);
                if (self.is_place_encoded_as_local_var(location)) {
                    stmts.push(
                        ast.local_var_assign(encoded_loc, encoded_value)
                    );
                } else {
                    stmts.push(
                        ast.field_assign(encoded_loc, encoded_value)
                    );
                }
                stmts.extend(effects_after);
                let target_cfg_block = cfg_blocks.get(&target).unwrap_or(&spec_cfg_block);
                (stmts, Successor::Goto(*target_cfg_block))
            },

            TerminatorKind::Call {
                ref args,
                ref destination,
                func: mir::Operand::Constant(
                    box mir::Constant {
                        literal: mir::Literal::Value {
                            value: &ty::Const {
                                val: ConstVal::Function(def_id, _),
                                ..
                            }
                        },
                        ..
                    }
                ),
                ..
            } => {
                let ast = self.encoder.ast_factory();
                let func_proc_name = self.encoder.env().get_item_name(def_id);
                if (func_proc_name == "prusti_contracts::internal::__assertion") {
                    // This is a Prusti loop invariant
                    panic!("Unreachable");
                } else if (func_proc_name == "std::rt::begin_panic") {
                    // This is called when a Rust assertion fails
                    stmts.push(ast.comment(&format!("Rust panic - {:?}", args[0])));
                    stmts.push(ast.assert(ast.false_lit(), ast.no_position()));
                } else {
                    let mut stmts_after: Vec<Stmt> = vec![];
                    let mut encoded_args: Vec<Expr> = vec![];

                    for operand in args.iter() {
                        let (encoded, effects_before, effects_after) = self.encode_operand(operand);
                        encoded_args.push(encoded);
                        stmts.extend(effects_before);
                        stmts_after.extend(effects_after);
                    }

                    let encoded_target: Vec<Expr> = destination.iter().map(|d| self.encode_place(&d.0).0).collect();

                    stmts.push(ast.method_call(
                        &self.encoder.encode_procedure_name(def_id),
                        &encoded_args,
                        &encoded_target
                    ));

                    stmts.extend(stmts_after);
                }

                if let &Some((_, target)) = destination {
                    let target_cfg_block = cfg_blocks.get(&target).unwrap_or(&spec_cfg_block);
                    (stmts, Successor::Goto(*target_cfg_block))
                } else {
                    (stmts, Successor::Unreachable)
                }
            },

            TerminatorKind::Call { .. } => {
                // Other kind of calls?
                unimplemented!()
            },

            TerminatorKind::Assert { ref cond, expected, ref target, .. } => {
                trace!("Assert cond '{:?}', expected '{:?}'", cond, expected);
                let mut cfg_targets: Vec<(Expr, CfgBlockIndex)> = vec![];
                let cond_var_name = self.cfg_method.add_fresh_local_var(ast.bool_type());
                let cond_var = ast.local_var(&cond_var_name, ast.bool_type());
                let (cond_tmp_val, effect_stmts) = self.eval_operand(cond);
                stmts.push(
                    ast.local_var_assign(
                        cond_var,
                        cond_tmp_val
                    )
                );
                stmts.extend(
                    effect_stmts
                );
                let viper_guard = if (expected) {
                    cond_var
                } else {
                    ast.not(cond_var)
                };
                let target_cfg_block = *cfg_blocks.get(&target).unwrap();
                (stmts, Successor::GotoSwitch(vec![(viper_guard, target_cfg_block)], abort_cfg_block))
            }

            TerminatorKind::Call { .. } |
            TerminatorKind::Resume |
            TerminatorKind::Assert { .. } |
            TerminatorKind::Yield { .. } |
            TerminatorKind::GeneratorDrop => unimplemented!("{:?}", term.kind),
        }
    }

    pub fn encode(mut self) -> Method<'v> {
        // TODO: Make this into a query on the encoder to handle nicely method calls.
        let procedure_contract = compute_procedure_contract(self.procedure, self.encoder.env().tcx());

        let ast = self.encoder.ast_factory();

        // Formal args
        let mut start_block_stmts: Vec<Stmt> = vec![
            ast.comment(&format!("========== start =========="))
        ];
        for local in self.mir.args_iter() {
            let tmp_name = self.cfg_method.add_fresh_formal_arg(ast.ref_type());
            let name = self.encode_local_var_name(local);
            self.cfg_method.add_local_var(&name, ast.ref_type());
            let tmp_formal_arg = ast.local_var(&tmp_name, ast.ref_type());
            let formal_arg = ast.local_var(&name, ast.ref_type());
            start_block_stmts.push(
                ast.comment(&format!("Initialize formal arguments"))
            );
            start_block_stmts.push(
                ast.local_var_assign(
                    formal_arg,
                    tmp_formal_arg
                )
            );
        }

        // Formal return
        for local in self.mir.local_decls.indices().take(1) {
            let name = self.encode_local_var_name(local);
            self.cfg_method.add_formal_return(&name, ast.ref_type())
        }

        // Local vars
        for local in self.mir.vars_and_temps_iter() {
            let name = self.encode_local_var_name(local);
            self.cfg_method.add_local_var(&name, ast.ref_type())
        }

        let mut cfg_blocks: HashMap<BasicBlockIndex, CfgBlockIndex> = HashMap::new();

        // Initialize CFG blocks
        let start_cfg_block = self.cfg_method.add_block("start", vec![], start_block_stmts);

        let mut first_cfg_block = true;
        self.procedure.walk_once_cfg(|bbi, _| {
            let cfg_block = self.cfg_method.add_block(&format!("{:?}", bbi), vec![], vec![
                ast.comment(&format!("========== {:?} ==========", bbi))
            ]);
            if first_cfg_block {
                self.cfg_method.set_successor(start_cfg_block, Successor::Goto(cfg_block));
                first_cfg_block = false;
            }
            cfg_blocks.insert(bbi, cfg_block);
        });

        let spec_cfg_block = self.cfg_method.add_block("spec", vec![], vec![
            ast.comment(&format!("========== spec ==========")),
            ast.comment("This should never be reached. It's a residual of type-checking specifications."),
        ]);
        self.cfg_method.set_successor(spec_cfg_block, Successor::Unreachable);

        let abort_cfg_block = self.cfg_method.add_block("abort", vec![], vec![
            ast.comment(&format!("========== abort ==========")),
            ast.comment("Target of any Rust panic."),
            ast.assert(ast.true_lit(), ast.no_position()),
        ]);
        self.cfg_method.set_successor(abort_cfg_block, Successor::Unreachable);

        let return_cfg_block = self.cfg_method.add_block("return", vec![], vec![
            ast.comment(&format!("========== return ==========")),
            ast.comment("Target of any 'return' statement."),
        ]);
        self.cfg_method.set_successor(return_cfg_block, Successor::Return);

        // Encode preconditions
        self.cfg_method.add_stmt(start_cfg_block, ast.comment("Preconditions:"));
        for local in self.mir.args_iter() {
            let ty = self.get_rust_local_ty(local);
            let predicate_name = self.encoder.encode_type_predicate_use(ty);
            let inhale_stmt = ast.inhale(
                ast.predicate_access_predicate(
                    ast.predicate_access(
                        &[
                            self.encode_local(local)
                        ],
                        &predicate_name
                    ),
                    ast.full_perm(),
                ),
                ast.no_position()
            );
            self.cfg_method.add_stmt(start_cfg_block, inhale_stmt);
        }
        self.cfg_method.add_stmt(start_cfg_block, ast.label("precondition", &[]));

        // Encode postcondition
        self.cfg_method.add_stmt(return_cfg_block, ast.comment("Postconditions:"));
        for local in self.mir.local_decls.indices().take(1) {
            let ty = self.get_rust_local_ty(local);
            let predicate_name = self.encoder.encode_type_predicate_use(ty);
            let exhale_stmt = ast.exhale(
                ast.predicate_access_predicate(
                    ast.predicate_access(
                        &[
                            self.encode_local(local)
                        ],
                        &predicate_name
                    ),
                    ast.full_perm(),
                ),
                ast.no_position()
            );
            self.cfg_method.add_stmt(return_cfg_block, exhale_stmt);
        }

        // Encode statements
        self.procedure.walk_once_cfg(|bbi, bb_data| {
            let statements: &Vec<mir::Statement<'tcx>> = &bb_data.statements;
            let mut viper_statements: Vec<Stmt> = vec![];

            // Encode statements
            for (stmt_index, stmt) in statements.iter().enumerate() {
                trace!("Encode statement {:?}:{}", bbi, stmt_index);
                let cfg_block = *cfg_blocks.get(&bbi).unwrap();
                self.cfg_method.add_stmt(cfg_block, ast.comment(&format!("{:?}", stmt)));
                let stmts = self.encode_statement(stmt);
                for stmt in stmts.into_iter() {
                    self.cfg_method.add_stmt(cfg_block, stmt);
                }
            }
        });

        // Encode terminators and set CFG edges
        self.procedure.walk_once_cfg(|bbi, bb_data| {
            if let Some(ref term) = bb_data.terminator {
                trace!("Encode terminator of {:?}", bbi);
                let cfg_block = *cfg_blocks.get(&bbi).unwrap();
                self.cfg_method.add_stmt(cfg_block, ast.comment(&format!("{:?}", term.kind)));
                let (stmts, successor) = self.encode_terminator(
                    term,
                    &cfg_blocks,
                    spec_cfg_block,
                    abort_cfg_block,
                    return_cfg_block
                );
                for stmt in stmts.into_iter() {
                    self.cfg_method.add_stmt(cfg_block, stmt);
                }
                self.cfg_method.set_successor(cfg_block, successor);
            }
        });

        self.cfg_method.to_ast().ok().unwrap()
    }

    fn get_rust_local_decl(&self, local: mir::Local) -> &mir::LocalDecl<'tcx> {
        &self.mir.local_decls[local]
    }

    fn get_rust_local_ty(&self, local: mir::Local) -> ty::Ty<'tcx> {
        self.get_rust_local_decl(local).ty
    }

    fn encode_local_var_name(&self, local: mir::Local) -> String {
        let local_decl = self.get_rust_local_decl(local);
        /*match local_decl.name {
            Some(ref name) => format!("{:?}", name),
            None => format!("{:?}", local)
        }*/
        format!("{:?}", local)
    }

    fn encode_local(&self, local: mir::Local) -> Expr<'v> {
        let var_name = self.encode_local_var_name(local);
        let var_type = self.encoder.ast_factory().ref_type();
        self.encoder.ast_factory().local_var(&var_name, var_type)
    }

    /// Returns
    /// - `Expr<'v>`: the expression of the projection;
    /// - `ty::Ty<'tcx>`: the type of the expression;
    /// - `Option<usize>`: optionally, the variant of the enum.
    fn encode_projection(&self, place_projection: &mir::PlaceProjection<'tcx>) -> (Expr<'v>, ty::Ty<'tcx>, Option<usize>) {
        debug!("Encode projection {:?}", place_projection);
        let (encoded_base, base_ty, opt_variant_index) = self.encode_place(&place_projection.base);
        let ast = self.encoder.ast_factory();
        match &place_projection.elem {
            &mir::ProjectionElem::Field(ref field, ty) => {
                match base_ty.sty {
                    ty::TypeVariants::TyBool |
                    ty::TypeVariants::TyInt(_) |
                    ty::TypeVariants::TyUint(_) |
                    ty::TypeVariants::TyRawPtr(_) |
                    ty::TypeVariants::TyRef(_, _) => panic!("Type {:?} has no fields", base_ty),

                    ty::TypeVariants::TyTuple(elems, _) => {
                        let field_name = format!("tuple_{}", field.index());
                        let field_ty = elems[field.index()];
                        let encoded_field = self.encoder.encode_ref_field(&field_name);
                        let encoded_projection = self.encoder.ast_factory().field_access(encoded_base, encoded_field);
                        (encoded_projection, field_ty, None)
                    },

                    ty::TypeVariants::TyAdt(ref adt_def, ref subst) => {
                        debug!("subst {:?}", subst);
                        let variant_index = opt_variant_index.unwrap_or(0);
                        let tcx = self.encoder.env().tcx();
                        assert!(variant_index as u64 == adt_def.discriminant_for_variant(tcx, variant_index).to_u64().unwrap());
                        let field = &adt_def.variants[variant_index].fields[field.index()];
                        let num_variants = adt_def.variants.len();
                        let field_name = if (num_variants == 1) {
                            format!("struct_{}", field.name)
                        } else {
                            format!("enum_{}_{}", variant_index, field.name)
                        };
                        let field_ty = tcx.type_of(field.did);
                        let encoded_field = self.encoder.encode_ref_field(&field_name);
                        let encoded_projection = self.encoder.ast_factory().field_access(encoded_base, encoded_field);
                        (encoded_projection, field_ty, None)
                    },

                    ref x => unimplemented!("{:?}", x),
                }
            },

            &mir::ProjectionElem::Deref => {
                match base_ty.sty {
                    ty::TypeVariants::TyRawPtr(ty::TypeAndMut { ty, .. }) |
                    ty::TypeVariants::TyRef(_, ty::TypeAndMut { ty, .. }) => {
                        let ref_field = self.encoder.encode_ref_field("val_ref");
                        let access = ast.field_access(
                            encoded_base,
                            ref_field
                        );
                        (access, ty, None)
                    },

                    _ => unreachable!(),
                }
            },

            &mir::ProjectionElem::Downcast(ref adt_def, variant_index) => {
                debug!("Downcast projection {:?}, {:?}", adt_def, variant_index);
                (encoded_base, base_ty, Some(variant_index))
            },

            x => unimplemented!("{:?}", x),
        }
    }

    /// Returns
    /// - `Expr<'v>`: the expression of the projection;
    /// - `ty::Ty<'tcx>`: the type of the expression;
    /// - `Option<usize>`: optionally, the variant of the enum.
    fn encode_place(&self, place: &mir::Place<'tcx>) -> (Expr<'v>, ty::Ty<'tcx>, Option<usize>) {
        match place {
            &mir::Place::Local(local) => (
                self.encode_local(local),
                self.get_rust_local_ty(local),
                None
            ),
            &mir::Place::Projection(ref place_projection) =>
                self.encode_projection(place_projection),
            x => unimplemented!("{:?}", x),
        }
    }

    fn eval_place(&mut self, place: &mir::Place<'tcx>) -> Expr<'v> {
        let (encoded_place, place_ty, opt_variant_index) = self.encode_place(place);
        let value_field = self.encoder.encode_value_field(place_ty);

        self.encoder.ast_factory().field_access(
            encoded_place,
            value_field
        )
    }

    fn is_place_encoded_as_local_var(&self, place: &mir::Place<'tcx>) -> bool {
        match place {
            &mir::Place::Local(local) => true,
            &mir::Place::Projection(_) => false,
            x => unimplemented!("{:?}", x),
        }
    }

    /// Returns:
    /// - an `Expr<'v>` that corresponds to the value of the operand;
    /// - a vector `Vec<Stmt<'v>>` of post side effects.
    fn eval_operand(&mut self, operand: &mir::Operand<'tcx>) -> (Expr<'v>, Vec<Stmt<'v>>) {
        let ast = self.encoder.ast_factory();
        match operand {
            &mir::Operand::Constant(box mir::Constant{ literal: mir::Literal::Value{ value: &ty::Const{ ref val, .. } }, ..}) => {
                (self.encoder.eval_const_val(val), vec![])
            }
            &mir::Operand::Copy(ref place) => {
                (self.eval_place(place), vec![])
            },
            &mir::Operand::Move(ref place) =>{
                let encoded_place = self.encode_place(place).0;
                let null_stmt  = ast.assign(
                    encoded_place,
                    ast.null_lit(),
                    self.is_place_encoded_as_local_var(place)
                );
                (self.eval_place(place), vec![null_stmt])
            },
            x => unimplemented!("{:?}", x)
        }
    }

    fn encode_operand(&mut self, operand: &mir::Operand<'tcx>) -> (Expr<'v>, Vec<Stmt<'v>>, Vec<Stmt<'v>>) {
        debug!("Encode operand {:?}", operand);
        let ast = self.encoder.ast_factory();
        match operand {
            &mir::Operand::Move(ref place) => {
                let encoded_place = self.encode_place(place).0;
                let stmt  = ast.assign(
                    encoded_place,
                    ast.null_lit(),
                    self.is_place_encoded_as_local_var(place)
                );
                (encoded_place, vec![], vec![stmt])
            },
            &mir::Operand::Copy(ref place) => {
                let fresh_var_name = self.cfg_method.add_fresh_local_var(ast.ref_type());
                let fresh_var = ast.local_var(&fresh_var_name, ast.ref_type());
                let (src, ty, opt_variant_index) = self.encode_place(place);
                let stmts = self.encode_copy(src, fresh_var, ty, true);
                (fresh_var, stmts, vec![])
            },
            &mir::Operand::Constant(box mir::Constant{ literal: mir::Literal::Value{ value: &ty::Const{ ref val, ty } }, ..}) => {
                let mut stmts: Vec<Stmt> = vec![];
                let fresh_var_name = self.cfg_method.add_fresh_local_var(ast.ref_type());
                let fresh_var = ast.local_var(&fresh_var_name, ast.ref_type());
                stmts.extend(
                    self.encode_allocation(fresh_var, ty, true)
                );
                let const_val = self.encoder.eval_const_val(val);
                let field = self.encoder.encode_value_field(ty);
                stmts.push(
                    ast.field_assign(ast.field_access(fresh_var, field), const_val)
                );
                (fresh_var, stmts, vec![])
            },
            x => unimplemented!("{:?}", x)
        }
    }

    fn encode_allocation(&mut self, dst: Expr<'v>, ty: ty::Ty<'tcx>, dst_is_local_var: bool) -> Vec<Stmt<'v>> {
        debug!("Encode allocation {:?}", ty);
        let ast = self.encoder.ast_factory();

        let field_name_type = self.encoder.encode_type_fields(ty);
        let fields: Vec<Field<'v>> = field_name_type
            .iter()
            .map(|x| self.encoder.encode_ref_field(&x.0))
            .collect();

        if (dst_is_local_var) {
            vec![
                ast.new_stmt(
                    dst,
                    &fields
                ),
            ]
        } else {
            let tmp_var_name = self.cfg_method.add_fresh_local_var(ast.ref_type());
            let tmp_var = ast.local_var(&tmp_var_name, ast.ref_type());

            vec![
                ast.new_stmt(
                    tmp_var,
                    &fields
                ),
                ast.field_assign(
                    dst,
                    tmp_var
                )
            ]
        }
    }

    fn encode_copy(&mut self, src: Expr<'v>, dst: Expr<'v>, self_ty: ty::Ty<'tcx>, dst_is_local_var: bool) -> Vec<Stmt<'v>> {
        debug!("Encode copy {:?}, {:?}", self_ty, dst_is_local_var);
        let ast = self.encoder.ast_factory();
        let mut stmts: Vec<Stmt> = vec![];

        stmts.extend(
            self.encode_allocation(dst, self_ty, dst_is_local_var)
        );

        let mut fields_name_ty = self.encoder.encode_type_fields(self_ty);

        match self_ty.sty {
            ty::TypeVariants::TyBool |
            ty::TypeVariants::TyInt(_) |
            ty::TypeVariants::TyUint(_) => {
                for (field_name, field, opt_field_ty) in fields_name_ty.drain(..) {
                    stmts.push(
                        ast.field_assign(
                            ast.field_access(dst, field),
                            ast.field_access(src, field)
                        )
                    );
                }
            }

            ty::TypeVariants::TyRawPtr(_) |
            ty::TypeVariants::TyRef(_, _) => {
                let ref_field = self.encoder.encode_ref_field("val_ref");
                stmts.push(
                    ast.field_assign(
                        ast.field_access(dst, ref_field),
                        ast.field_access(src, ref_field)
                    )
                );
            },

            _ => {
                for (field_name, field, opt_field_ty) in fields_name_ty.drain(..) {
                    let inner_src = ast.field_access(src, field);
                    let inner_dst = ast.field_access(dst, field);
                    if let Some(field_ty) = opt_field_ty {
                        stmts.extend(
                            self.encode_copy(inner_src, inner_dst, field_ty, false)
                        );
                    } else {
                        stmts.push(
                            ast.field_assign(
                                ast.field_access(dst, field),
                                ast.field_access(src, field)
                            )
                        );
                    }
                }
            }
        };

        stmts
    }

    fn encode_bin_op_value(&mut self, op: mir::BinOp, left: Expr<'v>, right: Expr<'v>) -> Expr<'v> {
        let ast = self.encoder.ast_factory();
        match op {
            mir::BinOp::Gt => ast.gt_cmp(left, right),

            mir::BinOp::Add => ast.add(left, right),

            mir::BinOp::Sub => ast.sub(left, right),

            x => unimplemented!("{:?}", x)
        }
    }

    fn encode_bin_op_check(&mut self, op: mir::BinOp, left: Expr<'v>, right: Expr<'v>) -> Expr<'v> {
        warn!("TODO: Encode bin op check {:?} ", op);
        // TODO
        self.encoder.ast_factory().true_lit()
    }

    fn encode_assign_aggregate(
        &mut self,
        ty: ty::Ty<'tcx>,
        aggregate: &mir::AggregateKind<'tcx>,
        operands: &Vec<mir::Operand<'tcx>>
    ) -> (Expr<'v>, Vec<Stmt<'v>>) {
        debug!("Encode aggregate {:?}, {:?}", aggregate, operands);
        let ast = self.encoder.ast_factory();
        let dst_var_name = self.cfg_method.add_fresh_local_var(ast.ref_type());
        let dst_var = ast.local_var(&dst_var_name, ast.ref_type());
        let mut stmts: Vec<Stmt> = vec![];
        stmts.extend(
            self.encode_allocation(dst_var, ty, true)
        );

        match aggregate {
            &mir::AggregateKind::Tuple => {
                for (field_num, operand) in operands.iter().enumerate() {
                    let field_name = format!("tuple_{}", field_num);
                    let encoded_field = self.encoder.encode_ref_field(&field_name);
                    let (encoded_operand, before_stmts, after_stmts) = self.encode_operand(operand);
                    stmts.extend(before_stmts);
                    stmts.push(
                        ast.field_assign(
                            ast.field_access(dst_var, encoded_field),
                            encoded_operand
                        )
                    );
                    stmts.extend(after_stmts);
                }
                (dst_var, stmts)
            },

            &mir::AggregateKind::Adt(adt_def, variant_index, substs, n) => {
                let num_variants = adt_def.variants.len();
                if num_variants > 1 {
                    let discr_field = self.encoder.encode_discriminant_field().1;
                    stmts.push(
                        ast.field_assign(
                            ast.field_access(dst_var, discr_field),
                            ast.int_lit(variant_index as i32)
                        )
                    );
                };
                let variant_def = &adt_def.variants[variant_index];
                for (field_index, field) in variant_def.fields.iter().enumerate() {
                    let operand = &operands[field_index];
                    let field_name = if (num_variants == 1) {
                        format!("struct_{}", field.name)
                    } else {
                        format!("enum_{}_{}", variant_index, field.name)
                    };
                    let encoded_field = self.encoder.encode_ref_field(&field_name);
                    let (encoded_operand, before_stmts, after_stmts) = self.encode_operand(operand);
                    stmts.extend(before_stmts);
                    stmts.push(
                        ast.field_assign(
                            ast.field_access(dst_var, encoded_field),
                            encoded_operand
                        )
                    );
                    stmts.extend(after_stmts);
                }
                (dst_var, stmts)
            },

            ref x => unimplemented!("{:?}", x)
        }
    }
}
