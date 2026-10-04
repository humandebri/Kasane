import ReleaseTimestampSequence
namespace ReleaseTimestampSequence
def releaseAdd : List Statement :=
  [.input 4, .asNanos 7, .narrow 6 7, .unwrap 5 6,
   .saturate .add 3 4 5, .returnTimestamp 3]
def releaseSub : List Statement :=
  [.input 4, .asNanos 7, .narrow 6 7, .unwrap 5 6,
   .saturate .sub 3 4 5, .returnTimestamp 3]
end ReleaseTimestampSequence
