#![allow(dead_code)]

use jni::JNIEnv;
use jni::sys::jint;
use jni::objects::JObject;
use jni_utils::JniUtils;
#[allow(unused_imports)]
use viper_sys::wrappers::*;
#[allow(unused_imports)]
use viper_sys::wrappers::viper::silver::ast;

pub struct AstFactory<'a> {
    env: &'a JNIEnv<'a>,
    jni: JniUtils<'a>,
}

macro_rules! jobject_wrapper {
    ($name:ident) => (
        pub struct $name<'a> { obj: JObject<'a> }
        impl<'a> $name<'a> {
            fn new(obj: JObject<'a>) -> Self {
                $name { obj }
            }
            pub fn to_jobject(&self) -> JObject {
                self.obj
            }
        }
    );
}

macro_rules! map_to_jobject {
    ($item:expr) => (
        $item.map(|x| x.to_jobject())
    );
}

macro_rules! map_to_jobjects {
    ($items:expr) => (
        map_to_jobject!($items.iter()).collect()
    );
}

jobject_wrapper!(Program);
jobject_wrapper!(Method);
jobject_wrapper!(Seqn);
jobject_wrapper!(Stmt);
jobject_wrapper!(Expr);
jobject_wrapper!(Position);

impl<'a> AstFactory<'a> {
    pub fn new(env: &'a JNIEnv) -> Self {
        let jni = JniUtils::new(env);
        AstFactory { env, jni }
    }

    pub fn new_no_position(&self) -> Position {
        let obj = self.jni.unwrap_result(
            ast::NoPosition_object::with(self.env).singleton(),
        );
        Position { obj }
    }

    pub fn new_line_column_position(&self, line: jint, column: jint) -> Position {
        let obj = self.jni.unwrap_result(
            ast::LineColumnPosition::with(self.env).new(
                line,
                column,
            ),
        );
        Position { obj }
    }

    pub fn new_identifier_position(&self, line: jint, column: jint, pos_id: &str) -> Position {
        let obj = self.jni.unwrap_result(
            ast::IdentifierPosition::with(self.env).new(
                self.jni.unwrap_result(
                    java::nio::file::Paths::with(self.env).call_get(
                        self.jni.new_string(""),
                        self.jni.new_object_array(0),
                    ),
                ),
                self.new_line_column_position(line, column).to_jobject(),
                self.jni.new_option(None),
                self.jni.new_string(pos_id),
            ),
        );
        Position { obj }
    }

    fn new_no_info(&self) -> JObject {
        self.jni.unwrap_result(
            ast::NoInfo_object::with(self.env).singleton(),
        )
    }

    fn new_simple_info(&self, comments: Vec<String>) -> JObject {
        self.jni.unwrap_result(ast::SimpleInfo::with(self.env).new(
            self.jni.new_seq(
                comments.iter().map(|x| self.jni.new_string(x)).collect(),
            ),
        ))
    }

    fn new_no_trafos(&self) -> JObject {
        self.jni.unwrap_result(
            ast::NoTrafos_object::with(self.env).singleton(),
        )
    }

    pub fn new_program(&self, methods: Vec<&Method>) -> Program<'a> {
        let obj = self.jni.unwrap_result(ast::Program::with(self.env).new(
            self.jni.new_seq(vec![]),
            self.jni.new_seq(vec![]),
            self.jni.new_seq(vec![]),
            self.jni.new_seq(vec![]),
            self.jni.new_seq(map_to_jobjects!(methods)),
            self.new_no_position().to_jobject(),
            self.new_no_info(),
            self.new_no_trafos(),
        ));
        Program { obj }
    }

    pub fn new_method(
        &self,
        name: &str,
        body: Option<&Seqn>,
        pres: Vec<&Expr>,
        posts: Vec<&Expr>,
    ) -> Method<'a> {
        let obj = self.jni.unwrap_result(ast::Method::with(self.env).new(
            self.jni.new_string(name),
            self.jni.new_seq(vec![]),
            self.jni.new_seq(vec![]),
            self.jni.new_seq(map_to_jobjects!(pres)),
            self.jni.new_seq(map_to_jobjects!(posts)),
            self.jni.new_option(body.map(|x| x.to_jobject())),
            self.new_no_position().to_jobject(),
            self.new_no_info(),
            self.new_no_trafos(),
        ));
        Method { obj }
    }

    pub fn new_seqn(&self, stmts: Vec<&Stmt>) -> Seqn<'a> {
        let obj = self.jni.unwrap_result(ast::Seqn::with(self.env).new(
            self.jni.new_seq(
                map_to_jobjects!(stmts),
            ),
            self.jni.new_seq(vec![]),
            self.new_no_position().to_jobject(),
            self.new_no_info(),
            self.new_no_trafos(),
        ));
        Seqn { obj }
    }

    pub fn new_assert(&self, expr: &Expr, pos: Position) -> Stmt<'a> {
        let obj = self.jni.unwrap_result(ast::Assert::with(self.env).new(
            expr.to_jobject(),
            pos.to_jobject(),
            self.new_no_info(),
            self.new_no_trafos(),
        ));
        Stmt { obj }
    }

    pub fn new_assert_with_comment(&self, expr: &Expr, pos: Position, comment: String) -> Stmt<'a> {
        let obj = self.jni.unwrap_result(ast::Assert::with(self.env).new(
            expr.to_jobject(),
            pos.to_jobject(),
            self.new_simple_info(
                vec![comment],
            ),
            self.new_no_trafos(),
        ));
        Stmt { obj }
    }

    pub fn new_true_lit(&self) -> Expr<'a> {
        let obj = self.jni.unwrap_result(ast::TrueLit::with(self.env).new(
            self.new_no_position().to_jobject(),
            self.new_no_info(),
            self.new_no_trafos(),
        ));
        Expr { obj }
    }

    pub fn new_false_lit(&self) -> Expr<'a> {
        let obj = self.jni.unwrap_result(ast::FalseLit::with(self.env).new(
            self.new_no_position().to_jobject(),
            self.new_no_info(),
            self.new_no_trafos(),
        ));
        Expr { obj }
    }
}
