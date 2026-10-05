//! Task column definitions. Geometry and cell values are owned separately.
use super::{column, AdaptiveColumnSpec, TableKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnId {
    Device,
    Selected,
    Relation,
    Index,
    Name,
    Time,
    Capacity,
    Reliability,
    VidPid,
    Model,
    Serial,
    Onlyid,
    User,
    Dept,
    ProvisionKind,
    Bus,
    State,
    Backups,
    Health,
    Partition,
    RoleOrState,
    Filesystem,
    ActionOrKey,
    FinalState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TableColumnSpec {
    pub id: ColumnId,
    pub heading: &'static str,
    pub layout: AdaptiveColumnSpec,
    pub copyable: bool,
}

fn table_column(
    id: ColumnId,
    heading: &'static str,
    layout: AdaptiveColumnSpec,
) -> TableColumnSpec {
    TableColumnSpec {
        id,
        heading,
        layout,
        copyable: true,
    }
}

fn table_control_column(
    id: ColumnId,
    heading: &'static str,
    layout: AdaptiveColumnSpec,
) -> TableColumnSpec {
    TableColumnSpec {
        id,
        heading,
        layout,
        copyable: false,
    }
}

pub fn identity_column_specs() -> [TableColumnSpec; 7] {
    use ColumnId::*;
    [
        table_column(Capacity, "容量", column(8, 9, 12, 80, 1, false)),
        table_column(VidPid, "VID:PID", column(9, 9, 12, 75, 1, false)),
        table_column(Model, "型号", column(4, 4, 16, 50, 2, false)),
        table_column(Onlyid, "onlyid", column(8, 12, 20, 70, 1, false)),
        table_column(User, "姓名", column(4, 6, 12, 45, 1, false)),
        table_column(Dept, "部门", column(8, 12, 20, 20, 3, false)),
        table_column(ProvisionKind, "盘型", column(12, 18, 22, 95, 1, true)),
    ]
}

pub fn table_column_schema(kind: TableKind) -> Option<Vec<TableColumnSpec>> {
    use ColumnId::*;
    let identity = identity_column_specs();
    match kind {
        TableKind::Devices => Some(vec![
            table_column(Device, "设备", column(7, 9, 12, 100, 1, true)),
            table_column(Capacity, "容量", column(8, 9, 12, 96, 1, true)),
            table_column(Dept, "部门", column(8, 16, 32, 94, 2, true)),
            table_column(User, "姓名", column(6, 10, 18, 93, 1, true)),
            table_column(ProvisionKind, "盘型", column(12, 22, 30, 98, 2, true)),
            table_column(State, "状态", column(8, 12, 18, 97, 1, true)),
            table_column(Backups, "备份", column(4, 6, 8, 70, 1, false)),
            table_column(Reliability, "身份可靠性", column(8, 10, 12, 69, 1, false)),
            table_column(Model, "型号", column(10, 18, 32, 68, 2, false)),
            table_column(VidPid, "VID:PID", column(9, 9, 12, 67, 1, false)),
            table_column(Serial, "序列号", column(10, 18, 32, 66, 2, false)),
        ]),
        TableKind::Backups => {
            let identity_column = |id| {
                *identity
                    .iter()
                    .find(|column| column.id == id)
                    .expect("backup identity column")
            };
            Some(vec![
                table_control_column(Selected, "选", column(3, 3, 4, 99, 1, true)),
                table_column(Index, "序号", column(4, 6, 8, 90, 1, true)),
                table_column(Time, "时间", column(12, 17, 20, 25, 1, false)),
                identity_column(Capacity),
                identity_column(Dept),
                identity_column(User),
                identity_column(Model),
                identity_column(ProvisionKind),
                table_column(Health, "健康", column(8, 11, 15, 97, 1, true)),
                identity_column(VidPid),
                identity_column(Onlyid),
                table_column(Name, "名称", column(10, 23, 48, 96, 2, true)),
            ])
        }
        TableKind::RelatedBackups => Some(vec![
            table_column(Relation, "关系", column(7, 9, 10, 100, 1, true)),
            table_column(Time, "时间", column(12, 17, 20, 95, 1, true)),
            table_column(Capacity, "容量", column(8, 10, 12, 90, 1, false)),
            table_column(Dept, "部门", column(8, 16, 32, 85, 2, false)),
            table_column(User, "姓名", column(6, 10, 18, 82, 1, false)),
            table_column(Model, "型号", column(10, 18, 32, 78, 2, false)),
            table_column(ProvisionKind, "盘型", column(12, 20, 28, 96, 1, true)),
            table_column(VidPid, "VID:PID", column(9, 9, 12, 75, 1, false)),
            table_column(Onlyid, "onlyid", column(8, 12, 20, 72, 1, false)),
            table_column(Name, "名称", column(12, 24, 48, 70, 3, false)),
        ]),
        TableKind::ResultPartitions => Some(vec![
            table_column(Partition, "分区", column(5, 6, 8, 100, 1, true)),
            table_column(RoleOrState, "角色/状态", column(8, 14, 18, 96, 1, true)),
            table_column(Filesystem, "文件系统", column(8, 12, 16, 90, 1, true)),
            table_column(Capacity, "容量", column(8, 12, 16, 86, 1, true)),
            table_column(ActionOrKey, "处理/密钥", column(10, 16, 24, 78, 1, false)),
            table_column(FinalState, "结果/说明", column(12, 24, 48, 72, 3, false)),
        ]),
        _ => None,
    }
}
